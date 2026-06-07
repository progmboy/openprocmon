//! Formatting for the 100-nanosecond `FILETIME` timestamps the driver reports
//! (cf. C++ `UtilConvertTimeOfDay` / `UtilConvertDay` / `UtilConvertTimeSpan`).

use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
use windows::Win32::System::Time::FileTimeToSystemTime;

/// 100-ns ticks per second.
const TICKS_PER_SECOND: i64 = 10_000_000;

/// Splits 100-ns ticks (since 1601 UTC) into local broken-down time.
fn to_local(ticks: i64) -> Option<SYSTEMTIME> {
    let utc = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut local = FILETIME::default();
    // SAFETY: both pointers reference valid `FILETIME` values for the calls.
    unsafe { FileTimeToLocalFileTime(&utc, &mut local) }.ok()?;
    let mut st = SYSTEMTIME::default();
    // SAFETY: `local` is valid; `st` receives the broken-down time.
    unsafe { FileTimeToSystemTime(&local, &mut st) }.ok()?;
    Some(st)
}

/// `HH:MM:SS.fffffff` in local time, falling back to the raw ticks if conversion
/// fails (cf. `UtilConvertTimeOfDay`).
pub fn time_of_day(ticks: i64) -> String {
    match to_local(ticks) {
        Some(st) => {
            let frac = ticks.rem_euclid(TICKS_PER_SECOND);
            format!(
                "{:02}:{:02}:{:02}.{:07}",
                st.wHour, st.wMinute, st.wSecond, frac
            )
        }
        None => ticks.to_string(),
    }
}

/// `YYYY/MM/DD HH:MM:SS` in local time (cf. `UtilConvertDay`).
pub fn date(ticks: i64) -> String {
    match to_local(ticks) {
        Some(st) => format!(
            "{:04}/{:02}/{:02} {:02}:{:02}:{:02}",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
        ),
        None => ticks.to_string(),
    }
}

/// Duration between two tick timestamps as `S.fffffff` seconds
/// (cf. `UtilConvertTimeSpan`).
pub fn duration(start: i64, end: i64) -> String {
    let delta = (end - start).max(0);
    format!(
        "{}.{:07}",
        delta / TICKS_PER_SECOND,
        delta % TICKS_PER_SECOND
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_formats_seconds_and_fraction() {
        // 1.5 seconds = 15_000_000 ticks.
        assert_eq!(duration(0, 15_000_000), "1.5000000");
        // Negative spans clamp to zero.
        assert_eq!(duration(100, 0), "0.0000000");
    }
}
