//! TDatime packing (TDatime.cxx):
//! `(year-1995)<<26 | month<<22 | day<<17 | hour<<12 | min<<6 | sec`.

/// Pack civil date/time fields into ROOT's TDatime word.
#[must_use]
pub fn pack_datime(year: u32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> u32 {
    (year.saturating_sub(1995)) << 26 | month << 22 | day << 17 | hour << 12 | min << 6 | sec
}

/// Current time (UTC) as a TDatime word.
///
/// ROOT/uproot use local time here; we use UTC to stay dependency-free.
/// The field is informational only ("file modified" shown by TBrowser).
#[must_use]
pub fn now_datime() -> u32 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    pack_datime(
        y as u32,
        m,
        d,
        (rem / 3600) as u32,
        (rem % 3600 / 60) as u32,
        (rem % 60) as u32,
    )
}

/// Days-since-epoch to (year, month, day); Howard Hinnant's civil_from_days.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_reference_datime() {
        // The vendored uproot reference file was written 2026-06-12 16:11:45
        // and carries fDatime 0x7d9902ed in every key.
        assert_eq!(pack_datime(2026, 6, 12, 16, 11, 45), 0x7d99_02ed);
    }

    #[test]
    fn civil_epoch_and_leap() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(20_616), (2026, 6, 12));
    }
}
