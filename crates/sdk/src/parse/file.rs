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

    /// The requested `CreateDisposition` (`Supersede`/`Open`/`Create`/…), from
    /// the high byte of the create `Options`. `None` if this isn't a create
    /// record or its parameters are truncated.
    fn create_disposition(&self) -> Option<&'static str> {
        if self.major() != Some(irp_mj::CREATE) {
            return None;
        }
        let params = Self::flt_params(self.ev.pre_data())?;
        // SAFETY: guarded on IRP_MJ_CREATE, so the `Create` arm is active.
        let options = unsafe { params.Create }.Options;
        Some(strings::file_create_disposition(options >> 24))
    }

    /// The completion `OpenResult` (`Created`/`Opened`/`Overwritten`/…), from the
    /// POST record's IO_STATUS `Information` field. `None` until the create
    /// completes (no POST) or for a non-create record.
    fn open_result(&self) -> Option<&'static str> {
        if self.major() != Some(irp_mj::CREATE) {
            return None;
        }
        // OpenResult comes from the completion's IO_STATUS Information field
        // (low 32 bits), cf. C++ `*(ULONG_PTR*)TO_EVENT_DATA(pPostEntry)`.
        let ret = self.ev.post_as::<LogFileCreatePost>()?.information as u32;
        Some(strings::file_ret_disposition(ret))
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

        let mut detail = format!(
            "Desired Access: {}, Disposition: {}, Options: {}, Attributes: {}, ShareMode: {}, AllocationSize: {}",
            strings::file_access_mask(desired_access),
            strings::file_create_disposition(create_disposition),
            strings::file_create_options(create_options),
            strings::file_attributes(file_attributes),
            strings::file_share_access(share_access),
            allocation_size,
        );
        if let Some(ret) = self.open_result() {
            detail.push_str(&format!(", OpenResult: {ret}"));
        }
        detail
    }

    /// A structured file field by name — one of [`FILE_FIELDS`]. `None` for an
    /// unknown name or a field that doesn't apply to this record (e.g.
    /// `OpenResult` before the create completes). The query layer's file
    /// extension fields, read straight from the record (no `Column` bloat).
    pub(crate) fn field(&self, name: &str) -> Option<&'static str> {
        match name {
            "Disposition" => self.create_disposition(),
            "OpenResult" => self.open_result(),
            _ => None,
        }
    }
}

/// The file extension fields exposed to the query layer: `(name, numeric,
/// description)`. Both are the CreateFile columns Procmon shows in its Detail;
/// exposing them lets a query classify *what a CreateFile did* (opened an
/// existing file vs created / overwrote one) without string-parsing Detail.
pub const FILE_FIELDS: &[(&str, bool, &str)] = &[
    (
        "Disposition",
        false,
        "CreateFile's requested disposition: Supersede / Open / Create / OpenIf / \
         Overwrite / OverwriteIf — what the caller asked for (vs OpenResult, what \
         happened).",
    ),
    (
        "OpenResult",
        false,
        "CreateFile's completion result: Created / Opened / Overwritten / Superseded / \
         Exists / DoesNotExist — what actually happened to the file. Group by this to \
         separate files a process created from ones it merely opened.",
    ),
];

impl FileView<'_> {
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

#[cfg(test)]
mod tests {
    use crate::event::Event;
    use crate::kernel_types::{file_opt, irp_mj, synth_record, LogFileOptHead, FILE_NOTIFY_BASE};
    use core::mem::size_of;
    use windows::Wdk::Storage::FileSystem::Minifilters::FLT_PARAMETERS;
    use windows::Win32::Storage::FileSystem::FILE_GENERIC_READ;

    /// Builds a live `IRP_MJ_CREATE` event: `disposition` is the requested
    /// CreateDisposition (high byte of Options); `information`, if `Some`, is the
    /// POST record's IO_STATUS Information (the OpenResult source).
    fn create_event(disposition: u32, information: Option<u64>) -> Event {
        let nt = "\\Device\\HarddiskVolume1\\Windows\\test.txt";
        let name: Vec<u8> = nt.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut d = vec![0u8; size_of::<LogFileOptHead>()];
        // SAFETY: FLT_PARAMETERS is POD for our purposes; zeroed is valid.
        let mut params: FLT_PARAMETERS = unsafe { core::mem::zeroed() };
        params.Create.Options = disposition << 24;
        // SAFETY: read the union's bytes for serialization.
        let pb = unsafe {
            core::slice::from_raw_parts(
                &params as *const _ as *const u8,
                size_of::<FLT_PARAMETERS>(),
            )
        };
        d.extend_from_slice(pb);
        d.extend_from_slice(&((name.len() / 2) as u16).to_le_bytes()); // NameLength
        d.extend_from_slice(&0u16.to_le_bytes()); // Fill
        d.extend_from_slice(&name);
        d.extend_from_slice(&FILE_GENERIC_READ.0.to_le_bytes()); // LOG_FILE_CREATE.DesiredAccess
        d.extend_from_slice(&0u32.to_le_bytes()); // UserTokenLength
        let _ = file_opt::name_offset();

        let op = FILE_NOTIFY_BASE + irp_mj::CREATE as u16;
        let pre = synth_record(3, op, 0, &d).into_boxed_slice();
        let post =
            information.map(|info| synth_record(0, op, 0, &info.to_le_bytes()).into_boxed_slice());
        Event::from_filter(pre, post, None).expect("event")
    }

    #[test]
    fn struct_fields_expose_disposition_and_open_result() {
        // disposition byte 3 => "OpenIf"; IO_STATUS Information 2 => "Created".
        let ev = create_event(3, Some(2));
        assert_eq!(ev.struct_field("Disposition").as_deref(), Some("OpenIf"));
        assert_eq!(ev.struct_field("OpenResult").as_deref(), Some("Created"));
        // Numeric accessor: these are enumerations, not numbers.
        assert_eq!(ev.struct_number("OpenResult"), None);
        assert_eq!(ev.struct_field("Nonexistent"), None);
    }

    #[test]
    fn open_result_absent_without_completion() {
        // A create with no POST record has a Disposition but no OpenResult yet.
        let ev = create_event(2 /* Create */, None);
        assert_eq!(ev.struct_field("Disposition").as_deref(), Some("Create"));
        assert_eq!(ev.struct_field("OpenResult"), None);
    }
}
