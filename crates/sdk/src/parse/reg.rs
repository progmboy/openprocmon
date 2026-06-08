//! Registry operation path and detail (cf. C++ `regopt.cxx`).
//!
//! Every registry record begins with a `KeyNameLength` (UTF-16 unit count); the
//! key name follows the fixed struct for that operation, so the name offset is
//! the size of the per-operation struct. The path is converted to hive form
//! (`HKLM\...`) here since that needs no system state.

use crate::event::Event;
use crate::kernel_types::reg_notify as rn;
use crate::kernel_types::{
    cast, LogRegCreateOpenKey, LogRegEnumerateKey, LogRegEnumerateValueKey, LogRegKeyOnly,
    LogRegLoadKey, LogRegPostCreateOpenKey, LogRegQueryKey, LogRegQueryValueKey, LogRegRenameKey,
    LogRegSetValueKey,
};
use crate::parse::{decode_utf16, read_detail_str, str_field_len, DetailMode, OperationView};
use crate::path::reg_internal_to_normal;
use crate::strings;
use core::mem::size_of;
use windows::Win32::System::Registry::{REG_DWORD, REG_EXPAND_SZ, REG_MULTI_SZ, REG_QWORD, REG_SZ};

/// Operation view over a registry record.
pub(crate) struct RegView<'a> {
    ev: &'a Event,
}

impl<'a> RegView<'a> {
    pub(crate) fn new(ev: &'a Event) -> Self {
        Self { ev }
    }

    /// Byte offset of the key name = size of the operation's fixed struct. The
    /// name (UTF-16) follows it; its unit count is the leading `KeyNameLength`.
    pub(crate) fn name_offset(notify: u16) -> Option<usize> {
        let size = match notify {
            rn::OPENKEYEX | rn::CREATEKEYEX => size_of::<LogRegCreateOpenKey>(),
            rn::QUERYVALUEKEY => size_of::<LogRegQueryValueKey>(),
            rn::ENUMERATEVALUEKEY => size_of::<LogRegEnumerateValueKey>(),
            rn::ENUMERATEKEY => size_of::<LogRegEnumerateKey>(),
            rn::SETINFORMATIONKEY => size_of::<crate::kernel_types::LogRegSetInformationKey>(),
            rn::QUERYKEY => size_of::<LogRegQueryKey>(),
            rn::SETVALUEKEY => size_of::<LogRegSetValueKey>(),
            rn::LOADKEY => size_of::<LogRegLoadKey>(),
            rn::RENAMEKEY => size_of::<LogRegRenameKey>(),
            // Operations carrying only a key name.
            rn::DELETEVALUEKEY
            | rn::UNLOADKEY
            | rn::DELETEKEY
            | rn::FLUSHKEY
            | rn::KEYHANDLECLOSE
            | rn::SETKEYSECURITY
            | rn::QUERYKEYSECURITY
            | rn::QUERYMULTIPLEVALUEKEY => size_of::<LogRegKeyOnly>(),
            _ => return None,
        };
        Some(size)
    }

    /// The leading raw `KeyNameLength` field (present on all registry records at
    /// offset 0). In PML its high bit is the ASCII flag — interpret via the mode.
    fn key_name_raw(data: &[u8]) -> u16 {
        data.get(0..2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .unwrap_or(0)
    }

    fn create_open_detail(&self, data: &[u8]) -> String {
        let desired = cast::<LogRegCreateOpenKey>(data)
            .map(|i| i.desired_access)
            .unwrap_or(0);
        match self.ev.post_as::<LogRegPostCreateOpenKey>() {
            Some(p) => {
                let (granted, disposition) = (p.granted_access, p.disposition);
                format!(
                    "Desired Access: {}, Granted Access: {}, Disposition: {}",
                    strings::reg_access_mask(desired),
                    strings::reg_access_mask(granted),
                    disposition_name(disposition)
                )
            }
            None => format!("Desired Access: {}", strings::reg_access_mask(desired)),
        }
    }

    fn set_value_detail(data: &[u8], mode: DetailMode) -> String {
        let Some(i) = cast::<LogRegSetValueKey>(data) else {
            return String::new();
        };
        let (value_type, data_size) = (i.value_type, i.data_size);
        // The copied value bytes trail the key name (`copy_size` of them).
        let (_, name_bytes) = str_field_len(i.key_name_length, mode);
        let value_off = size_of::<LogRegSetValueKey>() + name_bytes;
        let value = data
            .get(value_off..value_off + i.copy_size as usize)
            .map(|b| reg_value_data(value_type, b))
            .unwrap_or_default();
        format!(
            "Type: {}, Length: {}, Data: {}",
            strings::reg_value_type(value_type),
            data_size,
            value
        )
    }

    fn query_value_detail(data: &[u8]) -> String {
        let Some(i) = cast::<LogRegQueryValueKey>(data) else {
            return String::new();
        };
        let (length, class) = (i.length, i.key_value_information_class);
        format!(
            "Length: {length}, Class: {}",
            strings::key_value_information_class(class)
        )
    }

    fn enum_value_detail(data: &[u8]) -> String {
        let Some(i) = cast::<LogRegEnumerateValueKey>(data) else {
            return String::new();
        };
        let (index, length, class) = (i.index, i.length, i.key_value_information_class);
        format!(
            "Index: {index}, Length: {length}, Class: {}",
            strings::key_value_information_class(class)
        )
    }

    fn enum_key_detail(data: &[u8]) -> String {
        let Some(i) = cast::<LogRegEnumerateKey>(data) else {
            return String::new();
        };
        let (index, length, class) = (i.index, i.length, i.key_information_class);
        format!(
            "Index: {index}, Length: {length}, Class: {}",
            strings::key_information_class(class)
        )
    }

    fn query_key_detail(data: &[u8]) -> String {
        let Some(i) = cast::<LogRegQueryKey>(data) else {
            return String::new();
        };
        let (length, class) = (i.length, i.key_information_class);
        format!(
            "Length: {length}, Class: {}",
            strings::key_information_class(class)
        )
    }

    fn set_information_detail(data: &[u8]) -> String {
        let Some(i) = cast::<crate::kernel_types::LogRegSetInformationKey>(data) else {
            return String::new();
        };
        let (class, length) = (i.key_set_information_class, i.key_set_information_length);
        format!(
            "KeySetInformationClass: {}, Length: {length}",
            strings::key_set_information_class(class)
        )
    }

    /// The source file name that trails the key name for a load-key record.
    fn load_key_detail(data: &[u8], mode: DetailMode) -> String {
        let Some(info) = cast::<LogRegLoadKey>(data) else {
            return String::new();
        };
        let (_, key_bytes) = str_field_len(info.key_name_length, mode);
        let name_start = size_of::<LogRegLoadKey>() + key_bytes;
        let (src, _) = read_detail_str(data, name_start, info.source_file_length, mode);
        if src.is_empty() {
            String::new()
        } else {
            format!("SourceFile: {src}")
        }
    }

    /// The new key name that trails the old name for a rename-key record.
    fn rename_key_detail(data: &[u8], mode: DetailMode) -> String {
        let Some(info) = cast::<LogRegRenameKey>(data) else {
            return String::new();
        };
        let (_, key_bytes) = str_field_len(info.key_name_length, mode);
        let name_start = size_of::<LogRegRenameKey>() + key_bytes;
        let (new_name, _) = read_detail_str(data, name_start, info.new_name_length, mode);
        if new_name.is_empty() {
            String::new()
        } else if mode == DetailMode::Pml {
            format!("NewName: {new_name}")
        } else {
            format!("NewName: {}", reg_internal_to_normal(&new_name))
        }
    }
}

impl OperationView for RegView<'_> {
    fn path(&self) -> Option<String> {
        let data = self.ev.pre_data();
        let mode = self.ev.mode();
        let offset = Self::name_offset(self.ev.notify_type())?;
        let raw = Self::key_name_raw(data);
        let (name, _) = read_detail_str(data, offset, raw, mode);
        if name.is_empty() {
            return None;
        }
        // PML stores the already-normalized hive path; live needs conversion.
        Some(if mode == DetailMode::Pml {
            name
        } else {
            reg_internal_to_normal(&name)
        })
    }

    fn detail(&self) -> String {
        let data = self.ev.pre_data();
        let mode = self.ev.mode();
        match self.ev.notify_type() {
            rn::CREATEKEYEX | rn::OPENKEYEX => self.create_open_detail(data),
            rn::SETVALUEKEY => Self::set_value_detail(data, mode),
            rn::QUERYVALUEKEY => Self::query_value_detail(data),
            rn::ENUMERATEVALUEKEY => Self::enum_value_detail(data),
            rn::ENUMERATEKEY => Self::enum_key_detail(data),
            rn::QUERYKEY => Self::query_key_detail(data),
            rn::SETINFORMATIONKEY => Self::set_information_detail(data),
            rn::LOADKEY => Self::load_key_detail(data, mode),
            rn::RENAMEKEY => Self::rename_key_detail(data, mode),
            _ => String::new(),
        }
    }
}

/// Serializes this registry event's live detail blob into PML form: the driver
/// blob verbatim with the key name (and, for Rename/Load, the second path) moved
/// to hive/DOS form (see [`crate::parse::transcode`]).
pub(crate) fn pml_detail(ev: &Event) -> Option<Vec<u8>> {
    use crate::parse::transcode::{live_str, splice, u16_at, PathEdit};
    use core::mem::{offset_of, size_of};
    let data = ev.pre_data();
    if data.is_empty() {
        return None;
    }
    let notify = ev.notify_type();
    let mut edits = Vec::new();
    if let Some(name_off) = RegView::name_offset(notify) {
        // Primary key name: its length field is the leading `KeyNameLength` (0).
        let raw = u16_at(data, 0);
        let name = live_str(data, name_off, raw);
        if !name.is_empty() {
            edits.push(PathEdit {
                len_field_off: 0,
                data_off: name_off,
                raw_units: raw as usize,
                text: reg_internal_to_normal(&name),
            });
        }
    }
    // Operations carrying a second path-bearing string after the key name.
    match notify {
        rn::RENAMEKEY => {
            if let Some(i) = cast::<LogRegRenameKey>(data) {
                let new_off = size_of::<LogRegRenameKey>() + i.key_name_length as usize * 2;
                let new_name = live_str(data, new_off, i.new_name_length);
                if !new_name.is_empty() {
                    edits.push(PathEdit {
                        len_field_off: offset_of!(LogRegRenameKey, new_name_length),
                        data_off: new_off,
                        raw_units: i.new_name_length as usize,
                        text: reg_internal_to_normal(&new_name),
                    });
                }
            }
        }
        rn::LOADKEY => {
            if let Some(i) = cast::<LogRegLoadKey>(data) {
                let src_off = size_of::<LogRegLoadKey>() + i.key_name_length as usize * 2;
                let src = live_str(data, src_off, i.source_file_length);
                if !src.is_empty() {
                    edits.push(PathEdit {
                        len_field_off: offset_of!(LogRegLoadKey, source_file_length),
                        data_off: src_off,
                        raw_units: i.source_file_length as usize,
                        text: crate::path::nt_to_dos(&src),
                    });
                }
            }
        }
        _ => {}
    }
    Some(splice(data, edits))
}

/// Registry create/open disposition (`REG_CREATED_NEW_KEY` / `REG_OPENED_EXISTING_KEY`).
fn disposition_name(disposition: u32) -> &'static str {
    match disposition {
        1 => "REG_CREATED_NEW_KEY",
        2 => "REG_OPENED_EXISTING_KEY",
        _ => "<Unknown>",
    }
}

/// Renders the copied value bytes of a `RegSetValue` for the Detail column,
/// decoded per the value type (numbers/strings shown directly, binary as hex).
fn reg_value_data(value_type: u32, bytes: &[u8]) -> String {
    if value_type == REG_DWORD.0 {
        match bytes.get(0..4) {
            Some(b) => u32::from_le_bytes(b.try_into().unwrap()).to_string(),
            None => String::new(),
        }
    } else if value_type == REG_QWORD.0 {
        match bytes.get(0..8) {
            Some(b) => u64::from_le_bytes(b.try_into().unwrap()).to_string(),
            None => String::new(),
        }
    } else if value_type == REG_SZ.0
        || value_type == REG_EXPAND_SZ.0
        || value_type == REG_MULTI_SZ.0
    {
        decode_utf16(bytes)
    } else {
        // Binary or unknown: hex bytes, truncated for the column.
        const MAX: usize = 16;
        let shown = &bytes[..bytes.len().min(MAX)];
        let mut hex: Vec<String> = shown.iter().map(|b| format!("{b:02X}")).collect();
        if bytes.len() > MAX {
            hex.push("...".to_string());
        }
        hex.join(" ")
    }
}
