//! The machine's local time, as far as UZE needs it: where the local day
//! began.

/// How many seconds after local midnight the instant `unix_seconds` falls,
/// read from the machine's own zone rules, DST included; `None` where the
/// platform cannot convert it.
pub fn seconds_into_local_day(unix_seconds: u64) -> Option<u64> {
    imp::seconds_into_local_day(unix_seconds)
}

#[cfg(unix)]
mod imp {
    pub(super) fn seconds_into_local_day(unix_seconds: u64) -> Option<u64> {
        let time = libc::time_t::try_from(unix_seconds).ok()?;
        // SAFETY: `tm` is plain integers and a nullable pointer, for which
        // all-zero bytes are valid; `localtime_r` overwrites it below.
        let mut local: libc::tm = unsafe { std::mem::zeroed() };
        // SAFETY: both pointers are valid for the call, and `localtime_r`
        // writes only into the `tm` it is handed.
        if unsafe { libc::localtime_r(&time, &mut local) }.is_null() {
            return None;
        }
        u64::try_from(local.tm_hour * 3600 + local.tm_min * 60 + local.tm_sec).ok()
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::{
        Foundation::{FILETIME, SYSTEMTIME},
        System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime},
    };

    /// 100-nanosecond intervals between 1601-01-01 and the Unix epoch.
    const EPOCH_DIFFERENCE: u64 = 116_444_736_000_000_000;

    pub(super) fn seconds_into_local_day(unix_seconds: u64) -> Option<u64> {
        let ticks = unix_seconds
            .checked_mul(10_000_000)?
            .checked_add(EPOCH_DIFFERENCE)?;
        let file_time = FILETIME {
            dwLowDateTime: ticks as u32,
            dwHighDateTime: (ticks >> 32) as u32,
        };
        // SAFETY: zeroed SYSTEMTIMEs are valid outputs; every pointer
        // outlives its call.
        let (mut utc, mut local): (SYSTEMTIME, SYSTEMTIME) = unsafe { std::mem::zeroed() };
        // SAFETY: as above.
        if unsafe { FileTimeToSystemTime(&file_time, &mut utc) } == 0 {
            return None;
        }
        // SAFETY: a null zone means the machine's current one.
        if unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) } == 0 {
            return None;
        }
        Some(
            u64::from(local.wHour) * 3600
                + u64::from(local.wMinute) * 60
                + u64::from(local.wSecond),
        )
    }
}
