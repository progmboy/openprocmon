//! File operation path and detail (cf. C++ `fileopt.cxx`).
//!
//! A file record's data region is a [`LogFileOptHead`](crate::kernel_types::LogFileOptHead),
//! then the embedded `FLT_PARAMETERS` union, then `NameLength`/`Name` (UTF-16),
//! and for `IRP_MJ_CREATE` a trailing [`LogFileCreate`](crate::kernel_types::LogFileCreate).
//! Offsets past the union are derived from `size_of::<FLT_PARAMETERS>()` so the
//! union's width is never hard-coded.

use crate::event::Event;
use crate::kernel_types::{
    cast, file_opt, irp_mj, LogFileCreate, LogFileCreatePost, FILE_NOTIFY_BASE,
};
use crate::parse::{read_detail_str, str_field_len, DetailMode, OperationView};
use crate::strings;
use windows::Wdk::Storage::FileSystem::Minifilters::FLT_PARAMETERS;

/// Operation view over a file record.
pub(crate) struct FileView<'a> {
    ev: &'a Event,
}

impl<'a> FileView<'a> {
    pub(crate) fn new(ev: &'a Event) -> Self {
        Self { ev }
    }

    /// IRP major function for this record (`NotifyType - FILE_NOTIFY_BASE`).
    fn major(&self) -> Option<u8> {
        self.ev
            .notify_type()
            .checked_sub(FILE_NOTIFY_BASE)
            .filter(|m| *m <= u8::MAX as u16)
            .map(|m| m as u8)
    }

    /// The raw `NameLength` field. In PML its high bit is the ASCII flag, so the
    /// char/byte counts are derived via [`str_field_len`] with the event's mode.
    fn name_len_raw(data: &[u8]) -> u16 {
        let off = file_opt::name_length_offset();
        data.get(off..off + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .unwrap_or(0)
    }

    /// Reads the embedded `FLT_PARAMETERS` union by value (its pointer members
    /// are never dereferenced).
    fn flt_params(data: &[u8]) -> Option<FLT_PARAMETERS> {
        let off = file_opt::FLT_PARAMS_OFFSET;
        if data.len() < off + file_opt::FLT_PARAMS_SIZE {
            return None;
        }
        // SAFETY: bounds checked above; `FLT_PARAMETERS` is `Copy` POD for our
        // purposes and read unaligned, so any sufficiently large region is valid.
        Some(unsafe { (data.as_ptr().add(off) as *const FLT_PARAMETERS).read_unaligned() })
    }

    /// Detail for `IRP_MJ_CREATE`, mirroring Procmon's create columns.
    fn create_detail(&self, data: &[u8]) -> String {
        let params = match Self::flt_params(data) {
            Some(p) => p,
            None => return String::new(),
        };
        // SAFETY: this record is a create, so the `Create` arm is the active one.
        let create = unsafe { params.Create };
        // Copy fields to locals: `FLT_PARAMETERS` is packed, so referencing its
        // fields directly (e.g. in `format!`) would be an unaligned reference.
        let options = create.Options;
        let file_attributes = create.FileAttributes as u32;
        let share_access = create.ShareAccess as u32;
        let allocation_size = create.AllocationSize;
        let create_options = options & 0x00FF_FFFF;
        let create_disposition = options >> 24;

        // LOG_FILE_CREATE trails the name; its byte length is mode-dependent
        // (PML packs ASCII names 1 byte/char).
        let (_, name_bytes) = str_field_len(Self::name_len_raw(data), self.ev.mode());
        let name_end = file_opt::name_offset() + name_bytes;
        let desired_access = data
            .get(name_end..)
            .and_then(cast::<LogFileCreate>)
            .map(|c| c.desired_access)
            .unwrap_or(0);

        // OpenResult comes from the completion's IO_STATUS Information field
        // (low 32 bits), cf. C++ `*(ULONG_PTR*)TO_EVENT_DATA(pPostEntry)`.
        let open_result = self
            .ev
            .post_as::<LogFileCreatePost>()
            .map(|p| p.information as u32);

        let mut detail = format!(
            "Desired Access: {}, Disposition: {}, Options: {}, Attributes: {}, ShareMode: {}, AllocationSize: {}",
            strings::file_access_mask(desired_access),
            strings::file_create_disposition(create_disposition),
            strings::file_create_options(create_options),
            strings::file_attributes(file_attributes),
            strings::file_share_access(share_access),
            allocation_size,
        );
        if let Some(ret) = open_result {
            detail.push_str(&format!(
                ", OpenResult: {}",
                strings::file_ret_disposition(ret)
            ));
        }
        detail
    }

    /// Detail for read/write: byte offset and length from the FLT parameters.
    fn rw_detail(data: &[u8], is_write: bool) -> String {
        let params = match Self::flt_params(data) {
            Some(p) => p,
            None => return String::new(),
        };
        // SAFETY: the active union arm matches the record's major function.
        let (length, offset) = unsafe {
            if is_write {
                (params.Write.Length, params.Write.ByteOffset)
            } else {
                (params.Read.Length, params.Read.ByteOffset)
            }
        };
        format!("Offset: {offset}, Length: {length}")
    }

    /// Detail for set-information: the information class and buffer length.
    fn set_info_detail(data: &[u8]) -> String {
        let params = match Self::flt_params(data) {
            Some(p) => p,
            None => return String::new(),
        };
        // SAFETY: active arm matches IRP_MJ_SET_INFORMATION.
        let info = unsafe { params.SetFileInformation };
        let (class, length) = (info.FileInformationClass.0, info.Length);
        format!("FileInformationClass: {class}, Length: {length}")
    }

    /// Detail for set-security: the security information mask.
    fn set_security_detail(data: &[u8]) -> String {
        let params = match Self::flt_params(data) {
            Some(p) => p,
            None => return String::new(),
        };
        // SAFETY: active arm matches IRP_MJ_SET_SECURITY.
        let sec = unsafe { params.SetSecurity };
        format!(
            "Information: {}",
            strings::security_information(sec.SecurityInformation)
        )
    }

    /// Detail for query-information: the information class and buffer length.
    fn query_info_detail(data: &[u8]) -> String {
        let params = match Self::flt_params(data) {
            Some(p) => p,
            None => return String::new(),
        };
        // SAFETY: active arm matches IRP_MJ_QUERY_INFORMATION.
        let info = unsafe { params.QueryFileInformation };
        let (class, length) = (info.FileInformationClass.0, info.Length);
        format!("FileInformationClass: {class}, Length: {length}")
    }

    /// Detail for query-security: the security information mask.
    fn query_security_detail(data: &[u8]) -> String {
        let params = match Self::flt_params(data) {
            Some(p) => p,
            None => return String::new(),
        };
        // SAFETY: active arm matches IRP_MJ_QUERY_SECURITY.
        let sec = unsafe { params.QuerySecurity };
        format!(
            "Information: {}",
            strings::security_information(sec.SecurityInformation)
        )
    }

    /// Detail for device/IO control: the IOCTL control code.
    fn device_io_detail(data: &[u8]) -> String {
        let params = match Self::flt_params(data) {
            Some(p) => p,
            None => return String::new(),
        };
        // SAFETY: active arm matches IRP_MJ_DEVICE_CONTROL; `Common` aliases every
        // DeviceIoControl variant's leading `IoControlCode`.
        let dio = unsafe { params.DeviceIoControl.Common };
        format!("Control: 0x{:08X}", dio.IoControlCode)
    }

    /// Detail for byte-range lock/unlock: offset, key and exclusivity (`Length` is
    /// a kernel pointer in `FLT_PARAMETERS`, so it is not available here).
    fn lock_detail(data: &[u8]) -> String {
        let params = match Self::flt_params(data) {
            Some(p) => p,
            None => return String::new(),
        };
        // SAFETY: active arm matches IRP_MJ_LOCK_CONTROL.
        let lock = unsafe { params.LockControl };
        let (offset, key, exclusive) = (lock.ByteOffset, lock.Key, lock.ExclusiveLock.0 != 0);
        format!("Offset: {offset}, Key: {key}, ExclusiveLock: {exclusive}")
    }
}

impl OperationView for FileView<'_> {
    fn path(&self) -> Option<String> {
        let data = self.ev.pre_data();
        let mode = self.ev.mode();
        let raw = Self::name_len_raw(data);
        let (name, _) = read_detail_str(data, file_opt::name_offset(), raw, mode);
        if name.is_empty() {
            return None;
        }
        // Live records hold NT device paths (convert to DOS); PML already stores
        // the DOS path.
        Some(if mode == DetailMode::Pml {
            name
        } else {
            crate::path::nt_to_dos(&name)
        })
    }

    fn detail(&self) -> String {
        let data = self.ev.pre_data();
        match self.major() {
            Some(irp_mj::CREATE) => self.create_detail(data),
            Some(irp_mj::READ) => Self::rw_detail(data, false),
            Some(irp_mj::WRITE) => Self::rw_detail(data, true),
            Some(irp_mj::SET_INFORMATION) => Self::set_info_detail(data),
            Some(irp_mj::QUERY_INFORMATION) => Self::query_info_detail(data),
            Some(irp_mj::SET_SECURITY) => Self::set_security_detail(data),
            Some(irp_mj::QUERY_SECURITY) => Self::query_security_detail(data),
            Some(irp_mj::DEVICE_CONTROL) | Some(irp_mj::INTERNAL_DEVICE_CONTROL) => {
                Self::device_io_detail(data)
            }
            Some(irp_mj::LOCK_CONTROL) => Self::lock_detail(data),
            _ => String::new(),
        }
    }
}

/// Serializes this file event's live detail blob into PML form: the driver blob
/// verbatim with the file name relocated to its DOS path (see
/// [`crate::parse::transcode`]). Returns `None` if there is no detail to write.
pub(crate) fn pml_detail(ev: &Event) -> Option<Vec<u8>> {
    use crate::parse::transcode::{live_str, splice, u16_at, PathEdit};
    let data = ev.pre_data();
    if data.is_empty() {
        return None;
    }
    let len_off = file_opt::name_length_offset();
    let name_off = file_opt::name_offset();
    let raw = u16_at(data, len_off);
    let name = live_str(data, name_off, raw);
    let edits = if name.is_empty() {
        Vec::new()
    } else {
        vec![PathEdit {
            len_field_off: len_off,
            data_off: name_off,
            raw_units: raw as usize,
            text: crate::path::nt_to_dos(&name),
        }]
    };
    Some(splice(data, edits))
}
