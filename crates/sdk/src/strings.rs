//! Enum/flag to display-string mappings (cf. C++ `strmaps.cxx`, Process Monitor).
//!
//! Flag tables (access mask, attributes, create options, security info, registry
//! types) are keyed by named `windows` crate constants rather than magic numbers,
//! so they track the OS headers. The `NTSTATUS` table is keyed by the canonical
//! status values (its natural primary key, ported from Procmon's own lookup).
//! Display strings are Procmon's. Functions returning a single name yield
//! `&'static str` (zero allocation); flag formatters build a `String` only when
//! called.

use std::borrow::Cow;

use crate::event::Event;
use crate::kernel_types::{
    file_info_class, irp_mj, proc_notify, reg_notify, LogFileOptHead, MonitorType, FILE_NOTIFY_BASE,
};

use windows::Wdk::Storage::FileSystem::{
    FILE_COMPLETE_IF_OPLOCKED, FILE_CREATE_TREE_CONNECTION, FILE_DELETE_ON_CLOSE,
    FILE_DIRECTORY_FILE, FILE_DISALLOW_EXCLUSIVE, FILE_NON_DIRECTORY_FILE, FILE_NO_COMPRESSION,
    FILE_NO_EA_KNOWLEDGE, FILE_NO_INTERMEDIATE_BUFFERING, FILE_OPEN_BY_FILE_ID,
    FILE_OPEN_FOR_BACKUP_INTENT, FILE_OPEN_FOR_FREE_SPACE_QUERY, FILE_OPEN_NO_RECALL,
    FILE_OPEN_REPARSE_POINT, FILE_OPEN_REQUIRING_OPLOCK, FILE_RANDOM_ACCESS, FILE_RESERVE_OPFILTER,
    FILE_SEQUENTIAL_ONLY, FILE_SYNCHRONOUS_IO_ALERT, FILE_SYNCHRONOUS_IO_NONALERT,
    FILE_WRITE_THROUGH,
};
use windows::Wdk::System::Registry::{
    KeyBasicInformation, KeyCachedInformation, KeyFlagsInformation, KeyFullInformation,
    KeyHandleTagsInformation, KeyLayerInformation, KeyNameInformation, KeyNodeInformation,
    KeyTrustInformation, KeyValueBasicInformation, KeyValueFullInformation,
    KeyValueFullInformationAlign64, KeyValueLayerInformation, KeyValuePartialInformation,
    KeyValuePartialInformationAlign64, KeyVirtualizationInformation,
};
use windows::Win32::Security::{
    ATTRIBUTE_SECURITY_INFORMATION, BACKUP_SECURITY_INFORMATION, DACL_SECURITY_INFORMATION,
    GROUP_SECURITY_INFORMATION, LABEL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PROTECTED_SACL_SECURITY_INFORMATION,
    SACL_SECURITY_INFORMATION, SCOPE_SECURITY_INFORMATION, UNPROTECTED_DACL_SECURITY_INFORMATION,
    UNPROTECTED_SACL_SECURITY_INFORMATION,
};
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_ALL_ACCESS, FILE_APPEND_DATA, FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_COMPRESSED,
    FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_EA, FILE_ATTRIBUTE_ENCRYPTED,
    FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_INTEGRITY_STREAM, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_NOT_CONTENT_INDEXED, FILE_ATTRIBUTE_NO_SCRUB_DATA, FILE_ATTRIBUTE_OFFLINE,
    FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SPARSE_FILE,
    FILE_ATTRIBUTE_SYSTEM, FILE_ATTRIBUTE_TEMPORARY, FILE_DELETE_CHILD, FILE_EXECUTE,
    FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_READ_ATTRIBUTES,
    FILE_READ_DATA, FILE_READ_EA, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA, READ_CONTROL, SYNCHRONIZE, WRITE_DAC,
    WRITE_OWNER,
};
use windows::Win32::System::Registry::{
    KEY_ALL_ACCESS, KEY_CREATE_LINK, KEY_CREATE_SUB_KEY, KEY_ENUMERATE_SUB_KEYS, KEY_NOTIFY,
    KEY_QUERY_VALUE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_32KEY, KEY_WOW64_64KEY, KEY_WRITE,
    REG_BINARY, REG_DWORD, REG_DWORD_BIG_ENDIAN, REG_EXPAND_SZ, REG_LINK, REG_MULTI_SZ, REG_NONE,
    REG_QWORD, REG_SZ,
};
use windows::Win32::System::SystemServices::{ACCESS_SYSTEM_SECURITY, MAXIMUM_ALLOWED};

/// Joins the names of every flag set in `value`, appending leftover bits as hex.
/// Returns `"None"` when no flags are set, matching Procmon's display.
fn join_flags(value: u32, table: &[(u32, &'static str)]) -> String {
    let mut remaining = value;
    let mut parts: Vec<&str> = Vec::new();
    for &(mask, name) in table {
        if mask != 0 && remaining & mask == mask {
            parts.push(name);
            remaining &= !mask;
        }
    }
    if remaining != 0 {
        return if parts.is_empty() {
            format!("0x{remaining:x}")
        } else {
            format!("{}, 0x{remaining:x}", parts.join(", "))
        };
    }
    if parts.is_empty() {
        "None".to_string()
    } else {
        parts.join(", ")
    }
}

/// Maps an `NTSTATUS` to its Procmon result string, or `None` if unknown
/// (callers then fall back to the raw hex value).
///
/// The table is ported directly from Process Monitor's own status lookup
/// (reverse-engineered `sub_1400ABEB0`): the keys are the canonical `NTSTATUS`
/// numeric values and the text is Procmon's exact display string.
pub fn nt_status(status: i32) -> Option<&'static str> {
    // `0x...u32 as i32` keeps the canonical NTSTATUS value (high bit set).
    const TABLE: &[(i32, &str)] = &[
        (0x0000_0000u32 as i32, "SUCCESS"),
        (0x0000_0103u32 as i32, "PENDING"),
        (0x0000_0104u32 as i32, "REPARSE"),
        (0x0000_0105u32 as i32, "MORE ENTRIES"),
        (0x0000_0108u32 as i32, "OPLOCK BREAK IN PROGRESS"),
        (0x0000_010Bu32 as i32, "NOTIFY CLEANUP"),
        (0x0000_010Cu32 as i32, "NOTIFY ENUM DIR"),
        (0x0000_012Au32 as i32, "FILE LOCKED WITH ONLY READERS"),
        (0x0000_012Bu32 as i32, "FILE LOCKED WITH WRITERS"),
        (0x0000_0215u32 as i32, "OPLOCK SWITCHED TO NEW HANDLE"),
        (0x0000_0216u32 as i32, "OPLOCK HANDLE CLOSED"),
        (0x0000_0367u32 as i32, "WAIT FOR OPLOCK"),
        (0x4000_0016u32 as i32, "PREDEFINED HANDLE"),
        (0x8000_0002u32 as i32, "DATATYPE MISALIGNMENT"),
        (0x8000_0005u32 as i32, "BUFFER OVERFLOW"),
        (0x8000_0006u32 as i32, "NO MORE FILES"),
        (0x8000_0015u32 as i32, "INVALID EA FLAG"),
        (0x8000_001Au32 as i32, "NO MORE ENTRIES"),
        (0x8009_0322u32 as i32, "E_WRONG_PRINCIPAL"),
        (0xC000_0001u32 as i32, "UNSUCCESSFUL"),
        (0xC000_0002u32 as i32, "NOT IMPLEMENTED"),
        (0xC000_0003u32 as i32, "INVALID INFO CLASS"),
        (0xC000_0004u32 as i32, "INFO LENGTH MISMATCH"),
        (0xC000_0005u32 as i32, "ACCESS VIOLATION"),
        (0xC000_0006u32 as i32, "IN PAGE ERROR"),
        (0xC000_0008u32 as i32, "INVALID HANDLE"),
        (0xC000_000Du32 as i32, "INVALID PARAMETER"),
        (0xC000_000Eu32 as i32, "NO SUCH DEVICE"),
        (0xC000_000Fu32 as i32, "NO SUCH FILE"),
        (0xC000_0010u32 as i32, "INVALID DEVICE REQUEST"),
        (0xC000_0011u32 as i32, "END OF FILE"),
        (0xC000_0012u32 as i32, "WRONG VOLUME"),
        (0xC000_0013u32 as i32, "NO MEDIA"),
        (0xC000_0015u32 as i32, "NONEXISTENT SECTOR"),
        (0xC000_0017u32 as i32, "NO MEMORY"),
        (0xC000_0021u32 as i32, "ALREADY COMMITTED"),
        (0xC000_0022u32 as i32, "ACCESS DENIED"),
        (0xC000_0023u32 as i32, "BUFFER TOO SMALL"),
        (0xC000_0024u32 as i32, "OBJECT TYPE MISMATCH"),
        (0xC000_0032u32 as i32, "DISK CORRUPT"),
        (0xC000_0033u32 as i32, "NAME INVALID"),
        (0xC000_0034u32 as i32, "NAME NOT FOUND"),
        (0xC000_0035u32 as i32, "NAME COLLISION"),
        (0xC000_0039u32 as i32, "OBJECT PATH INVALID"),
        (0xC000_003Au32 as i32, "PATH NOT FOUND"),
        (0xC000_003Bu32 as i32, "PATH SYNTAX BAD"),
        (0xC000_003Cu32 as i32, "DATA OVERRUN"),
        (0xC000_003Fu32 as i32, "CRC ERROR"),
        (0xC000_0043u32 as i32, "SHARING VIOLATION"),
        (0xC000_0044u32 as i32, "QUOTA EXCEEDED"),
        (0xC000_004Fu32 as i32, "EAS NOT SUPPORTED"),
        (0xC000_0050u32 as i32, "EA TOO LARGE"),
        (0xC000_0051u32 as i32, "NONEXISTENT EA ENTRY"),
        (0xC000_0052u32 as i32, "NO EAS ON FILE"),
        (0xC000_0053u32 as i32, "EA CORRUPT ERROR"),
        (0xC000_0054u32 as i32, "FILE LOCK CONFLICT"),
        (0xC000_0055u32 as i32, "NOT GRANTED"),
        (0xC000_0056u32 as i32, "DELETE PENDING"),
        (0xC000_0061u32 as i32, "PRIVILEGE NOT HELD"),
        (0xC000_006Du32 as i32, "LOGON FAILURE"),
        (0xC000_007Eu32 as i32, "RANGE NOT LOCKED"),
        (0xC000_007Fu32 as i32, "DISK FULL"),
        (0xC000_0098u32 as i32, "FILE INVALID"),
        (0xC000_009Au32 as i32, "INSUFFICIENT RESOURCES"),
        (0xC000_009Cu32 as i32, "DEVICE DATA ERROR"),
        (0xC000_009Du32 as i32, "DEVICE NOT CONNECTED"),
        (0xC000_00A2u32 as i32, "MEDIA WRITE PROTECTED"),
        (0xC000_00A5u32 as i32, "BAD IMPERSONATION"),
        (0xC000_00ABu32 as i32, "INSTANCE NOT AVAILABLE"),
        (0xC000_00ACu32 as i32, "PIPE NOT AVAILABLE"),
        (0xC000_00ADu32 as i32, "INVALID PIPE STATE"),
        (0xC000_00AEu32 as i32, "PIPE BUSY"),
        (0xC000_00B0u32 as i32, "PIPE DISCONNECTED"),
        (0xC000_00B1u32 as i32, "PIPE CLOSING"),
        (0xC000_00B2u32 as i32, "PIPE CONNECTED"),
        (0xC000_00B3u32 as i32, "PIPE LISTENING"),
        (0xC000_00B4u32 as i32, "INVALID READ MODE"),
        (0xC000_00B5u32 as i32, "IO TIMEOUT"),
        (0xC000_00BAu32 as i32, "IS DIRECTORY"),
        (0xC000_00BBu32 as i32, "NOT SUPPORTED"),
        (0xC000_00BDu32 as i32, "DUPLICATE NAME"),
        (0xC000_00BEu32 as i32, "BAD NETWORK PATH"),
        (0xC000_00C1u32 as i32, "TOO MANY COMMANDS"),
        (0xC000_00C3u32 as i32, "INVALID NETWORK RESPONSE"),
        (0xC000_00C4u32 as i32, "NETWORK ERROR"),
        (0xC000_00CCu32 as i32, "BAD NETWORK NAME"),
        (0xC000_00D4u32 as i32, "NOT SAME DEVICE"),
        (0xC000_00D8u32 as i32, "CANT WAIT"),
        (0xC000_00D9u32 as i32, "PIPE EMPTY"),
        (0xC000_00DBu32 as i32, "CSC OBJECT PATH NOT FOUND"),
        (0xC000_00E2u32 as i32, "OPLOCK NOT GRANTED"),
        (0xC000_00EFu32 as i32, "INVALID PARAMETER 1"),
        (0xC000_00F0u32 as i32, "INVALID PARAMETER 2"),
        (0xC000_00F1u32 as i32, "INVALID PARAMETER 3"),
        (0xC000_00F2u32 as i32, "INVALID PARAMETER 4"),
        (0xC000_00FBu32 as i32, "REDIRECTOR NOT STARTED"),
        (0xC000_0101u32 as i32, "NOT EMPTY"),
        (0xC000_0102u32 as i32, "FILE CORRUPT"),
        (0xC000_0103u32 as i32, "NOT A DIRECTORY"),
        (0xC000_0107u32 as i32, "FILES OPEN"),
        (0xC000_010Du32 as i32, "CANNOT IMPERSONATE"),
        (0xC000_0120u32 as i32, "CANCELLED"),
        (0xC000_0121u32 as i32, "CANNOT DELETE"),
        (0xC000_0123u32 as i32, "FILE DELETED"),
        (0xC000_0128u32 as i32, "FILE CLOSED"),
        (0xC000_012Au32 as i32, "THREAD NOT IN PROCESS"),
        (0xC000_0148u32 as i32, "INVALID LEVEL"),
        (0xC000_014Bu32 as i32, "PIPE BROKEN"),
        (0xC000_014Cu32 as i32, "REGISTRY CORRUPT"),
        (0xC000_014Du32 as i32, "IO FAILED"),
        (0xC000_017Cu32 as i32, "KEY DELETED"),
        (0xC000_0181u32 as i32, "CHILD MUST BE VOLATILE"),
        (0xC000_0184u32 as i32, "INVALID DEVICE STATE"),
        (0xC000_0185u32 as i32, "IO DEVICE ERROR"),
        (0xC000_0188u32 as i32, "LOG FILE FULL"),
        (0xC000_019Cu32 as i32, "FS DRIVER REQUIRED"),
        (0xC000_0203u32 as i32, "USER SESSION DELETED"),
        (0xC000_0205u32 as i32, "INSUFFICIENT SERVER RESOURCES"),
        (0xC000_0207u32 as i32, "INVALID ADDRESS COMPONENT"),
        (0xC000_020Cu32 as i32, "DISCONNECTED"),
        (0xC000_0225u32 as i32, "NOT FOUND"),
        (0xC000_0243u32 as i32, "USER MAPPED FILE"),
        (0xC000_0248u32 as i32, "LOGIN WKSTA RESTRICTION"),
        (0xC000_0257u32 as i32, "PATH NOT COVERED"),
        (0xC000_026Du32 as i32, "DFS UNAVAILABLE"),
        (0xC000_0273u32 as i32, "NO MORE MATCHES"),
        (0xC000_0275u32 as i32, "NOT REPARSE POINT"),
        (0xC000_02EAu32 as i32, "CANNOT MAKE"),
        (0xC000_02F0u32 as i32, "OBJECTID NOT FOUND"),
        (0xC000_0388u32 as i32, "DOWNGRADE DETECTED"),
        (0xC000_0425u32 as i32, "HIVE UNLOADED"),
        (0xC000_0427u32 as i32, "FILE SYSTEM LIMITATION"),
        (0xC000_0463u32 as i32, "DEVICE FEATURE NOT SUPPORTED"),
        (0xC000_046Du32 as i32, "OBJECT NOT EXTERNALLY BACKED"),
        (0xC000_04ADu32 as i32, "STORAGE RESERVE ID INVALID"),
        (0xC000_04AEu32 as i32, "STORAGE RESERVE DOES NOT EXIST"),
        (0xC000_04AFu32 as i32, "STORAGE RESERVE ALREADY EXISTS"),
        (0xC000_04B0u32 as i32, "STORAGE RESERVE NOT EMPTY"),
        (0xC000_04B1u32 as i32, "NOT A DAX VOLUME"),
        (0xC000_0909u32 as i32, "CANNOT BREAK OPLOCK"),
        (0xC000_A2A1u32 as i32, "OFFLOAD READ FLT NOT SUPPORTED"),
        (0xC000_A2A2u32 as i32, "OFFLOAD WRITE FLT NOT SUPPORTED"),
        (0xC000_A2A3u32 as i32, "OFFLOAD READ FILE NOT SUPPORTED"),
        (0xC000_A2A4u32 as i32, "OFFLOAD WRITE FILE NOT SUPPORTED"),
        (0xC019_0001u32 as i32, "TRANSACTIONAL CONFLICT"),
        (0xC019_0002u32 as i32, "INVALID TRANSACTION"),
        (0xC019_0003u32 as i32, "TRANSACTION NOT ACTIVE"),
        (0xC019_003Eu32 as i32, "EFS NOT ALLOWED IN TRANSACTION"),
        (0xC019_003Fu32 as i32, "TRANSACTIONAL OPEN NOT ALLOWED"),
        (
            0xC019_0040u32 as i32,
            "TRANSACTED MAPPING UNSUPPORTED REMOTE",
        ),
        (0xC019_0044u32 as i32, "CANNOT EXECUTE FILE IN TRANSACTION"),
        (0xC019_0049u32 as i32, "SPARSE NOT ALLOWED IN TRANSACTION"),
        (0xC01C_0004u32 as i32, "FAST IO DISALLOWED"),
        // Extra codes Procmon's PML uses (cf. procmon-parser); kept here so PML
        // result names match and there is a single NTSTATUS name table.
        (0x8000_0011u32 as i32, "NO MORE ENTRIES"),
        (0xC000_006Au32 as i32, "WRONG PASSWORD"),
        (0xC000_0079u32 as i32, "INVALID SID"),
        (0xC000_01E5u32 as i32, "FAST IO DISALLOWED"),
    ];
    TABLE.iter().find(|&&(s, _)| s == status).map(|&(_, n)| n)
}

/// The result string for a status: its Procmon name, or `0x%X` hex when not in
/// the table. Mirrors Procmon's `sub_1400ACA90` (table lookup, else `sprintf`).
pub fn nt_status_string(status: i32) -> Cow<'static, str> {
    match nt_status(status) {
        Some(name) => Cow::Borrowed(name),
        None => Cow::Owned(format!("0x{:X}", status as u32)),
    }
}

/// Display name of an event category (cf. `StrMapClassEvent`).
pub fn class_event(monitor: MonitorType) -> &'static str {
    match monitor {
        MonitorType::Process => "Process",
        MonitorType::File => "File System",
        MonitorType::Reg => "Registry",
        MonitorType::Profiling => "Profiling",
        MonitorType::Post => "Post",
        MonitorType::Unknown(_) => "Unknown",
    }
}

/// Operation display name for an event (cf. C++ `StrMapOperation(PLOG_ENTRY)`).
///
/// Takes the whole event — the Rust analog of `PLOG_ENTRY` — and derives the
/// class, notify type, and (for file records) the IRP minor function itself,
/// just as the C++ function reads `pEntry->MonitorType`/`NotifyType` and
/// `TO_EVENT_DATA(PLOG_FILE_OPT, pEntry)->MinorFunction`.
pub fn operation(ev: &Event) -> &'static str {
    if let Some(net) = ev.network() {
        return crate::parse::network::op_label(net.is_tcp, net.op);
    }
    match ev.monitor_type() {
        MonitorType::Process => process_operation(ev.notify_type()),
        MonitorType::Reg => reg_operation(ev.notify_type()),
        MonitorType::File => {
            let minor = ev
                .pre_as::<LogFileOptHead>()
                .map_or(0, |opt| opt.minor_function);
            file_operation(ev.notify_type(), minor)
        }
        MonitorType::Profiling => "Profiling Event",
        _ => "<Unknown>",
    }
}

/// Profiling operation name (PML `ProfilingOperation`, a separate event class
/// from process notifies). 0=Thread, 1=Process, 2=Debug Output.
pub fn profiling_operation(code: u16) -> &'static str {
    match code {
        0 => "Thread Profiling",
        1 => "Process Profiling",
        2 => "Debug Output Profiling",
        _ => "<Unknown Profiling Op>",
    }
}

/// Process operation name from a `LOG_PROCESS_NOTIFY_TYPE`.
pub fn process_operation(notify: u16) -> &'static str {
    match notify {
        proc_notify::INIT => "Process Defined",
        proc_notify::CREATE => "Process Create",
        proc_notify::EXIT => "Process Exit",
        proc_notify::THREAD_CREATE => "Thread Create",
        proc_notify::THREAD_EXIT => "Thread Exit",
        proc_notify::IMAGE_LOAD => "Load Image",
        proc_notify::START => "Process Start",
        proc_notify::PERFORMANCE => "Process Profiling",
        proc_notify::THREAD_PERFORMANCE => "Thread Profiling",
        proc_notify::SYSTEM_PERFORMANCE => "System Profiling",
        _ => "<Unknown>",
    }
}

/// Registry operation name from a `LOG_REG_NOTIFY_TYPE`.
pub fn reg_operation(notify: u16) -> &'static str {
    match notify {
        reg_notify::OPENKEYEX => "RegOpenKey",
        reg_notify::CREATEKEYEX => "RegCreateKey",
        reg_notify::KEYHANDLECLOSE => "RegCloseKey",
        reg_notify::QUERYKEY => "RegQueryKey",
        reg_notify::SETVALUEKEY => "RegSetValue",
        reg_notify::QUERYVALUEKEY => "RegQueryValue",
        reg_notify::ENUMERATEVALUEKEY => "RegEnumValue",
        reg_notify::ENUMERATEKEY => "RegEnumKey",
        reg_notify::SETINFORMATIONKEY => "RegSetInfoKey",
        reg_notify::DELETEKEY => "RegDeleteKey",
        reg_notify::DELETEVALUEKEY => "RegDeleteValue",
        reg_notify::FLUSHKEY => "RegFlushKey",
        reg_notify::LOADKEY => "RegLoadKey",
        reg_notify::UNLOADKEY => "RegUnloadKey",
        reg_notify::RENAMEKEY => "RegRenameKey",
        reg_notify::QUERYMULTIPLEVALUEKEY => "RegQueryMultipleValue",
        reg_notify::SETKEYSECURITY => "RegSetKeySecurity",
        reg_notify::QUERYKEYSECURITY => "RegQueryKeySecurity",
        _ => "<Unknown>",
    }
}

/// File operation name from a file `NotifyType` (= IRP major function + base).
pub fn file_operation(notify: u16, minor: u8) -> &'static str {
    // The driver stores `NotifyType = (UCHAR)(MajorFunction + FILE_NOTIFY_BASE)`
    // truncated to 8 bits, so the major is recovered with a wrapping u8 subtract
    // (cf. C++ `(UCHAR)Operator - 20`). This is what keeps the fast-I/O pseudo
    // majors (0xFF, 0xFE, …) — which would otherwise wrap below the base — intact.
    let major = (notify as u8).wrapping_sub(FILE_NOTIFY_BASE as u8);
    // Majors whose displayed name is refined by the minor function (the
    // information class / fast-I/O lock variant), mirroring C++ `gFileOptMap`.
    let sub_table: Option<&[(u8, &str)]> = match major {
        irp_mj::READ => Some(file_info_class::READ),
        irp_mj::WRITE => Some(file_info_class::WRITE),
        irp_mj::QUERY_INFORMATION => Some(file_info_class::QUERY),
        irp_mj::SET_INFORMATION => Some(file_info_class::SET),
        irp_mj::QUERY_VOLUME_INFORMATION => Some(file_info_class::VOLUME),
        irp_mj::DIRECTORY_CONTROL => Some(file_info_class::DIRECTORY),
        irp_mj::LOCK_CONTROL => Some(file_info_class::LOCK),
        irp_mj::PNP => Some(file_info_class::PNP),
        _ => None,
    };
    if let Some(table) = sub_table {
        // A `0xFF` entry is a wildcard matching any minor (cf. C++ sub-map loop).
        if let Some(&(_, name)) = table
            .iter()
            .find(|&&(m, _)| m == file_info_class::SUB_WILDCARD || m == minor)
        {
            return name;
        }
        // Unknown minor: fall back to the major's generic name below.
    }
    file_major_name(major)
}

/// Generic display name for an IRP major function (cf. `gFileOptMap` show names),
/// including the fast-I/O / cache-manager pseudo majors.
fn file_major_name(major: u8) -> &'static str {
    match major {
        irp_mj::CREATE => "CreateFile",
        irp_mj::CREATE_NAMED_PIPE => "CreatePipe",
        irp_mj::CLOSE => "IRP_MJ_CLOSE",
        irp_mj::READ => "ReadFile",
        irp_mj::WRITE => "WriteFile",
        irp_mj::QUERY_INFORMATION => "QueryInformationFile",
        irp_mj::SET_INFORMATION => "SetInformationFile",
        irp_mj::QUERY_EA => "QueryEAFile",
        irp_mj::SET_EA => "SetEAFile",
        irp_mj::FLUSH_BUFFERS => "FlushBuffersFile",
        irp_mj::QUERY_VOLUME_INFORMATION => "QueryVolumeInformation",
        irp_mj::SET_VOLUME_INFORMATION => "SetVolumeInformation",
        irp_mj::DIRECTORY_CONTROL => "DirectoryControl",
        irp_mj::FILE_SYSTEM_CONTROL => "FileSystemControl",
        irp_mj::DEVICE_CONTROL => "DeviceIoControl",
        irp_mj::INTERNAL_DEVICE_CONTROL => "InternalDeviceIoControl",
        irp_mj::SHUTDOWN => "Shutdown",
        irp_mj::LOCK_CONTROL => "LockUnlockFile",
        irp_mj::CLEANUP => "CloseFile",
        irp_mj::CREATE_MAILSLOT => "CreateMailSlot",
        irp_mj::QUERY_SECURITY => "QuerySecurityFile",
        irp_mj::SET_SECURITY => "SetSecurityFile",
        irp_mj::POWER => "Power",
        irp_mj::SYSTEM_CONTROL => "SystemControl",
        irp_mj::DEVICE_CHANGE => "DeviceChange",
        irp_mj::QUERY_QUOTA => "QueryFileQuota",
        irp_mj::SET_QUOTA => "SetFileQuota",
        irp_mj::PNP => "PlugAndPlay",
        irp_mj::ACQUIRE_FOR_SECTION_SYNCHRONIZATION => "CreateFileMapping",
        irp_mj::RELEASE_FOR_SECTION_SYNCHRONIZATION => "ReleaseForSectionSync",
        irp_mj::ACQUIRE_FOR_MOD_WRITE => "AcquireForModWrite",
        irp_mj::RELEASE_FOR_MOD_WRITE => "ReleaseForModWrite",
        irp_mj::ACQUIRE_FOR_CC_FLUSH => "AcquireForCcFlush",
        irp_mj::RELEASE_FOR_CC_FLUSH => "ReleaseForCcFlush",
        irp_mj::QUERY_OPEN | irp_mj::NETWORK_QUERY_OPEN => "QueryOpen",
        irp_mj::FAST_IO_CHECK_IF_POSSIBLE => "FastIoCheckIfPossible",
        irp_mj::MDL_READ => "ReadFile",
        irp_mj::MDL_READ_COMPLETE => "MdlReadComplete",
        irp_mj::PREPARE_MDL_WRITE => "WriteFile",
        irp_mj::MDL_WRITE_COMPLETE => "MdlWriteComplete",
        irp_mj::VOLUME_MOUNT => "VolumeMount",
        irp_mj::VOLUME_DISMOUNT => "VolumeDismount",
        _ => "<Unknown>",
    }
}

/// Registry value type name (cf. `REG_*`).
pub fn reg_value_type(value_type: u32) -> &'static str {
    const TABLE: &[(u32, &str)] = &[
        (REG_NONE.0, "REG_NONE"),
        (REG_SZ.0, "REG_SZ"),
        (REG_EXPAND_SZ.0, "REG_EXPAND_SZ"),
        (REG_BINARY.0, "REG_BINARY"),
        (REG_DWORD.0, "REG_DWORD"),
        (REG_DWORD_BIG_ENDIAN.0, "REG_DWORD_BIG_ENDIAN"),
        (REG_LINK.0, "REG_LINK"),
        (REG_MULTI_SZ.0, "REG_MULTI_SZ"),
        (REG_QWORD.0, "REG_QWORD"),
    ];
    TABLE
        .iter()
        .find(|&&(t, _)| t == value_type)
        .map(|&(_, n)| n)
        .unwrap_or("REG_UNKNOWN")
}

/// Formats a registry `ACCESS_MASK` (cf. Procmon's registry Desired/Granted
/// Access). Composites (`All Access`, `Read`, `Write`) collapse first.
pub fn reg_access_mask(mask: u32) -> String {
    let table: &[(u32, &str)] = &[
        (KEY_ALL_ACCESS.0, "All Access"),
        (KEY_READ.0, "Read"), // == KEY_EXECUTE
        (KEY_WRITE.0, "Write"),
        (KEY_QUERY_VALUE.0, "Query Value"),
        (KEY_SET_VALUE.0, "Set Value"),
        (KEY_CREATE_SUB_KEY.0, "Create Sub Key"),
        (KEY_ENUMERATE_SUB_KEYS.0, "Enumerate Sub Keys"),
        (KEY_NOTIFY.0, "Notify"),
        (KEY_CREATE_LINK.0, "Create Link"),
        (KEY_WOW64_64KEY.0, "WOW64 64-Key"),
        (KEY_WOW64_32KEY.0, "WOW64 32-Key"),
        (DELETE.0, "Delete"),
        (READ_CONTROL.0, "Read Control"),
        (WRITE_DAC.0, "Write DAC"),
        (WRITE_OWNER.0, "Write Owner"),
        (SYNCHRONIZE.0, "Synchronize"),
    ];
    join_flags(mask, table)
}

/// Name of a `KEY_INFORMATION_CLASS` (the class queried by `RegQueryKey` /
/// `RegEnumerateKey`).
pub fn key_information_class(class: u32) -> &'static str {
    const TABLE: &[(i32, &str)] = &[
        (KeyBasicInformation.0, "KeyBasicInformation"),
        (KeyNodeInformation.0, "KeyNodeInformation"),
        (KeyFullInformation.0, "KeyFullInformation"),
        (KeyNameInformation.0, "KeyNameInformation"),
        (KeyCachedInformation.0, "KeyCachedInformation"),
        (KeyFlagsInformation.0, "KeyFlagsInformation"),
        (
            KeyVirtualizationInformation.0,
            "KeyVirtualizationInformation",
        ),
        (KeyHandleTagsInformation.0, "KeyHandleTagsInformation"),
        (KeyTrustInformation.0, "KeyTrustInformation"),
        (KeyLayerInformation.0, "KeyLayerInformation"),
    ];
    let c = class as i32;
    TABLE
        .iter()
        .find(|&&(k, _)| k == c)
        .map(|&(_, n)| n)
        .unwrap_or("<Unknown>")
}

/// Name of a `KEY_SET_INFORMATION_CLASS` (the class set by `RegSetInfoKey`).
pub fn key_set_information_class(class: u32) -> &'static str {
    match class {
        0 => "KeyWriteTimeInformation",
        1 => "KeyWow64FlagsInformation",
        2 => "KeyControlFlagsInformation",
        3 => "KeySetVirtualizationInformation",
        4 => "KeySetDebugInformation",
        5 => "KeySetHandleTagsInformation",
        _ => "<Unknown>",
    }
}

/// Name of a `KEY_VALUE_INFORMATION_CLASS` (the class queried by
/// `RegQueryValue` / `RegEnumerateValue`).
pub fn key_value_information_class(class: u32) -> &'static str {
    const TABLE: &[(i32, &str)] = &[
        (KeyValueBasicInformation.0, "KeyValueBasicInformation"),
        (KeyValueFullInformation.0, "KeyValueFullInformation"),
        (KeyValuePartialInformation.0, "KeyValuePartialInformation"),
        (
            KeyValueFullInformationAlign64.0,
            "KeyValueFullInformationAlign64",
        ),
        (
            KeyValuePartialInformationAlign64.0,
            "KeyValuePartialInformationAlign64",
        ),
        (KeyValueLayerInformation.0, "KeyValueLayerInformation"),
    ];
    let c = class as i32;
    TABLE
        .iter()
        .find(|&&(k, _)| k == c)
        .map(|&(_, n)| n)
        .unwrap_or("<Unknown>")
}

/// Formats a file `DesiredAccess` mask. Display names and ordering follow
/// Process Monitor (composites first so e.g. `Generic Read` collapses its bits).
pub fn file_access_mask(mask: u32) -> String {
    let table: &[(u32, &str)] = &[
        (FILE_ALL_ACCESS.0, "All Access"),
        (
            FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | FILE_GENERIC_EXECUTE.0,
            "Generic Read/Write/Execute",
        ),
        (
            FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
            "Generic Read/Write",
        ),
        (
            FILE_GENERIC_READ.0 | FILE_GENERIC_EXECUTE.0,
            "Generic Read/Execute",
        ),
        (
            FILE_GENERIC_WRITE.0 | FILE_GENERIC_EXECUTE.0,
            "Generic Write/Execute",
        ),
        (FILE_GENERIC_READ.0, "Generic Read"),
        (FILE_GENERIC_WRITE.0, "Generic Write"),
        (FILE_GENERIC_EXECUTE.0, "Generic Execute"),
        (FILE_READ_DATA.0, "Read Data/List Directory"),
        (FILE_WRITE_DATA.0, "Write Data/Add File"),
        (
            FILE_APPEND_DATA.0,
            "Append Data/Add Subdirectory/Create Pipe Instance",
        ),
        (FILE_READ_EA.0, "Read EA"),
        (FILE_WRITE_EA.0, "Write EA"),
        (FILE_EXECUTE.0, "Execute/Traverse"),
        (FILE_DELETE_CHILD.0, "Delete Child"),
        (FILE_READ_ATTRIBUTES.0, "Read Attributes"),
        (FILE_WRITE_ATTRIBUTES.0, "Write Attributes"),
        (DELETE.0, "Delete"),
        (READ_CONTROL.0, "Read Control"),
        (WRITE_DAC.0, "Write DAC"),
        (WRITE_OWNER.0, "Write Owner"),
        (SYNCHRONIZE.0, "Synchronize"),
        (ACCESS_SYSTEM_SECURITY, "Access System Security"),
        (MAXIMUM_ALLOWED, "Maximum Allowed"),
    ];
    join_flags(mask, table)
}

/// Formats a file `ShareAccess` mask (cf. `StrMapFileShareAccess`).
pub fn file_share_access(mask: u32) -> String {
    let table: &[(u32, &str)] = &[
        (FILE_SHARE_READ.0, "Read"),
        (FILE_SHARE_WRITE.0, "Write"),
        (FILE_SHARE_DELETE.0, "Delete"),
    ];
    join_flags(mask, table)
}

/// Formats a file `FileAttributes` mask. Names follow Process Monitor.
pub fn file_attributes(mask: u32) -> String {
    let table: &[(u32, &str)] = &[
        (FILE_ATTRIBUTE_READONLY.0, "Readonly"),
        (FILE_ATTRIBUTE_HIDDEN.0, "Hidden"),
        (FILE_ATTRIBUTE_SYSTEM.0, "System"),
        (FILE_ATTRIBUTE_DIRECTORY.0, "Directory"),
        (FILE_ATTRIBUTE_ARCHIVE.0, "Archive"),
        (FILE_ATTRIBUTE_DEVICE.0, "Device"),
        (FILE_ATTRIBUTE_NORMAL.0, "Normal"),
        (FILE_ATTRIBUTE_TEMPORARY.0, "Temporary"),
        (FILE_ATTRIBUTE_SPARSE_FILE.0, "Sparse"),
        (FILE_ATTRIBUTE_REPARSE_POINT.0, "Reparse Point"),
        (FILE_ATTRIBUTE_COMPRESSED.0, "Compressed"),
        (FILE_ATTRIBUTE_OFFLINE.0, "Offline"),
        (FILE_ATTRIBUTE_NOT_CONTENT_INDEXED.0, "Not Content Indexed"),
        (FILE_ATTRIBUTE_ENCRYPTED.0, "Encrypted"),
        (FILE_ATTRIBUTE_INTEGRITY_STREAM.0, "Integrity Stream"),
        (FILE_ATTRIBUTE_NO_SCRUB_DATA.0, "No Scrub Data"),
        (FILE_ATTRIBUTE_EA.0, "Extended Attributes"),
    ];
    join_flags(mask, table)
}

/// Formats the low 24 bits of a create `Options` field. Names follow Process
/// Monitor (`gFileCreateOptions`).
pub fn file_create_options(options: u32) -> String {
    let table: &[(u32, &str)] = &[
        (FILE_DIRECTORY_FILE.0, "Directory"),
        (FILE_WRITE_THROUGH.0, "Write Through"),
        (FILE_SEQUENTIAL_ONLY.0, "Sequential Access"),
        (FILE_NO_INTERMEDIATE_BUFFERING.0, "No Buffering"),
        (FILE_SYNCHRONOUS_IO_ALERT.0, "Synchronous IO Alert"),
        (FILE_SYNCHRONOUS_IO_NONALERT.0, "Synchronous IO Non-Alert"),
        (FILE_NON_DIRECTORY_FILE.0, "Non-Directory File"),
        (FILE_CREATE_TREE_CONNECTION.0, "Create Tree Connection"),
        (FILE_COMPLETE_IF_OPLOCKED.0, "Complete If Oplocked"),
        (FILE_NO_EA_KNOWLEDGE.0, "No EA Knowledge"),
        (FILE_RANDOM_ACCESS.0, "Random Access"),
        (FILE_DELETE_ON_CLOSE.0, "Delete On Close"),
        (FILE_OPEN_BY_FILE_ID.0, "Open By ID"),
        (FILE_OPEN_FOR_BACKUP_INTENT.0, "Open For Backup"),
        (FILE_NO_COMPRESSION.0, "No Compression"),
        (FILE_OPEN_REQUIRING_OPLOCK.0, "Open Requiring Oplock"),
        (FILE_DISALLOW_EXCLUSIVE.0, "Disallow Exclusive"),
        (FILE_RESERVE_OPFILTER.0, "Reserve OpFilter"),
        (FILE_OPEN_REPARSE_POINT.0, "Open Reparse Point"),
        (FILE_OPEN_NO_RECALL.0, "Open No Recall"),
        (
            FILE_OPEN_FOR_FREE_SPACE_QUERY.0,
            "Open For Free Space Query",
        ),
    ];
    join_flags(options, table)
}

/// Create disposition name from the high 8 bits of `Options`
/// (cf. `StrMapFileCreateDisposition`). Values follow `FILE_SUPERSEDE`..`FILE_OVERWRITE_IF`.
pub fn file_create_disposition(disposition: u32) -> &'static str {
    match disposition {
        0 => "Supersede",
        1 => "Open",
        2 => "Create",
        3 => "OpenIf",
        4 => "Overwrite",
        5 => "OverwriteIf",
        _ => "<Unknown>",
    }
}

/// Result disposition name reported by a create completion
/// (cf. `StrMapFileRetDisposition`). Same ordering as create disposition.
pub fn file_ret_disposition(disposition: u32) -> &'static str {
    match disposition {
        0 => "Superseded",
        1 => "Opened",
        2 => "Created",
        3 => "Overwritten",
        4 => "Exists",
        5 => "DoesNotExist",
        _ => "<Unknown>",
    }
}

/// Formats a `SECURITY_INFORMATION` mask (cf. `StrMapSecurityInformation`).
pub fn security_information(info: u32) -> String {
    let table: &[(u32, &str)] = &[
        (OWNER_SECURITY_INFORMATION.0, "Owner"),
        (GROUP_SECURITY_INFORMATION.0, "Group"),
        (DACL_SECURITY_INFORMATION.0, "DACL"),
        (SACL_SECURITY_INFORMATION.0, "SACL"),
        (LABEL_SECURITY_INFORMATION.0, "Label"),
        (ATTRIBUTE_SECURITY_INFORMATION.0, "Attribute"),
        (SCOPE_SECURITY_INFORMATION.0, "Scope"),
        (BACKUP_SECURITY_INFORMATION.0, "Backup"),
        (PROTECTED_DACL_SECURITY_INFORMATION.0, "Protected DACL"),
        (PROTECTED_SACL_SECURITY_INFORMATION.0, "Protected SACL"),
        (UNPROTECTED_DACL_SECURITY_INFORMATION.0, "Unprotected DACL"),
        (UNPROTECTED_SACL_SECURITY_INFORMATION.0, "Unprotected SACL"),
    ];
    join_flags(info, table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_names() {
        assert_eq!(nt_status(0), Some("SUCCESS"));
        assert_eq!(nt_status(0xC000_0034u32 as i32), Some("NAME NOT FOUND"));
        assert_eq!(nt_status(0xC000_0022u32 as i32), Some("ACCESS DENIED"));
        assert_eq!(nt_status(0x1234_5678), None);
    }

    #[test]
    fn status_string_falls_back_to_hex() {
        assert_eq!(nt_status_string(0), "SUCCESS");
        // Unknown status -> uppercase `0x%X` of the unsigned value (cf. Procmon).
        assert_eq!(nt_status_string(0x1234_5678), "0x12345678");
        assert_eq!(nt_status_string(0xC000_9999u32 as i32), "0xC0009999");
    }

    #[test]
    fn file_operation_minor_refinement() {
        let set_info = FILE_NOTIFY_BASE + irp_mj::SET_INFORMATION as u16;
        assert_eq!(file_operation(set_info, 0x0a), "SetRenameInformationFile");
        // Unknown minor falls back to the generic major name.
        assert_eq!(file_operation(set_info, 0xfe), "SetInformationFile");

        let lock = FILE_NOTIFY_BASE + irp_mj::LOCK_CONTROL as u16;
        assert_eq!(file_operation(lock, 1), "LockFile");

        let dir = FILE_NOTIFY_BASE + irp_mj::DIRECTORY_CONTROL as u16;
        assert_eq!(file_operation(dir, 2), "NotifyChangeDirectory");

        // Fast-I/O majors truncate below the base on the wire (the driver does
        // `(UCHAR)(major + 20)`); recovery must wrap. ACQUIRE_FOR_SECTION_SYNC
        // (0xFF) -> wire NotifyType (UCHAR)(0xFF + 20) == 19.
        assert_eq!(file_operation(19, 0), "CreateFileMapping");
        let mount = (FILE_NOTIFY_BASE + irp_mj::VOLUME_MOUNT as u16) & 0xff;
        assert_eq!(file_operation(mount, 0), "VolumeMount");

        // PnP minor refinement.
        let pnp = FILE_NOTIFY_BASE + irp_mj::PNP as u16;
        assert_eq!(file_operation(pnp, 0), "StartDevice");

        // Read/Write carry a `0xFF` wildcard entry: any minor maps to the verb.
        let read = FILE_NOTIFY_BASE + irp_mj::READ as u16;
        assert_eq!(file_operation(read, 0), "ReadFile");
        assert_eq!(file_operation(read, 0x7c), "ReadFile");
        let write = FILE_NOTIFY_BASE + irp_mj::WRITE as u16;
        assert_eq!(file_operation(write, 0x10), "WriteFile");
    }

    #[test]
    fn operation_dispatch() {
        use crate::event::Event;
        use crate::kernel_types::test_support::entry_bytes;
        let ev = |monitor, notify| {
            Event::from_filter(
                entry_bytes(monitor, notify, 1, 0, &[]).into_boxed_slice(),
                None,
                None,
            )
            .unwrap()
        };
        assert_eq!(operation(&ev(3, FILE_NOTIFY_BASE)), "CreateFile");
        assert_eq!(operation(&ev(2, reg_notify::SETVALUEKEY)), "RegSetValue");
        assert_eq!(operation(&ev(1, proc_notify::CREATE)), "Process Create");
    }

    #[test]
    fn flags_join_and_none() {
        let mask = FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0;
        assert_eq!(file_share_access(mask), "Read, Write");
        assert_eq!(file_share_access(0), "None");
    }

    #[test]
    fn access_mask_collapses_composites() {
        // A generic-read mask collapses to "Generic Read", not its component bits.
        assert_eq!(file_access_mask(FILE_GENERIC_READ.0), "Generic Read");
        // A single specific right shows its friendly name.
        assert_eq!(
            file_access_mask(FILE_READ_DATA.0),
            "Read Data/List Directory"
        );
    }

    #[test]
    fn security_information_extended() {
        assert_eq!(security_information(LABEL_SECURITY_INFORMATION.0), "Label");
        let both = OWNER_SECURITY_INFORMATION.0 | DACL_SECURITY_INFORMATION.0;
        assert_eq!(security_information(both), "Owner, DACL");
    }

    #[test]
    fn reg_value_types() {
        assert_eq!(reg_value_type(REG_SZ.0), "REG_SZ");
        assert_eq!(reg_value_type(0xdead), "REG_UNKNOWN");
    }

    #[test]
    fn reg_access_and_info_classes() {
        assert_eq!(reg_access_mask(KEY_READ.0), "Read");
        assert_eq!(
            reg_access_mask(KEY_QUERY_VALUE.0 | KEY_SET_VALUE.0),
            "Query Value, Set Value"
        );
        assert_eq!(
            key_information_class(KeyNameInformation.0 as u32),
            "KeyNameInformation"
        );
        assert_eq!(
            key_value_information_class(KeyValueFullInformation.0 as u32),
            "KeyValueFullInformation"
        );
    }
}
