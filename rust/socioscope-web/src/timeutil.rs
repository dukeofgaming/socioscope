//! UTC timestamps for OKF, and America/Chicago labels for the library.

#[derive(Clone, Copy)]
struct Civil {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

pub fn utc_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    format_utc(secs)
}

pub fn stamp_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let whole = (secs / 1000) as i64;
    let millis = (secs % 1000) as u32;
    let civil = civil_from_unix(whole);
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}{:03}Z",
        civil.year, civil.month, civil.day, civil.hour, civil.minute, civil.second, millis
    )
}

pub fn format_utc(secs: i64) -> String {
    let civil = civil_from_unix(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        civil.year, civil.month, civil.day, civil.hour, civil.minute, civil.second
    )
}

/// Library clock in America/Chicago. US Central daylight time is applied.
pub fn format_chicago(iso_utc: &str) -> String {
    let Some(secs) = parse_utc(iso_utc) else {
        return iso_utc.to_string();
    };
    let offset_hours: i64 = if central_is_daylight(secs) { 5 } else { 6 };
    let local = civil_from_unix(secs - offset_hours * 3600);
    let hour = local.hour % 12;
    let hour = if hour == 0 { 12 } else { hour };
    let suffix = if local.hour < 12 { "AM" } else { "PM" };
    format!(
        "{} {}, {} · {}:{:02} {} CT",
        MONTHS[local.month as usize - 1],
        local.day,
        local.year,
        hour,
        local.minute,
        suffix
    )
}

pub fn log_date(iso_utc: &str) -> String {
    iso_utc.get(..10).unwrap_or(iso_utc).to_string()
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn parse_utc(iso: &str) -> Option<i64> {
    let bytes = iso.as_bytes();
    if bytes.len() < 20 || bytes[10] != b'T' || !iso.ends_with('Z') {
        return None;
    }
    let year: i64 = iso[0..4].parse().ok()?;
    let month: u32 = iso[5..7].parse().ok()?;
    let day: u32 = iso[8..10].parse().ok()?;
    let hour: u32 = iso[11..13].parse().ok()?;
    let minute: u32 = iso[14..16].parse().ok()?;
    let second: u32 = iso[17..19].parse().ok()?;
    let days = days_from_civil(year as i32, month, day)?;
    Some(days * 86400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64)
}

fn civil_from_unix(secs: i64) -> Civil {
    let days = secs.div_euclid(86400);
    let sod = secs.rem_euclid(86400) as u32;
    let (year, month, day) = civil_from_days(days);
    Civil {
        year,
        month,
        day,
        hour: sod / 3600,
        minute: (sod % 3600) / 60,
        second: sod % 60,
    }
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let y = year as i64 - if month <= 2 { 1 } else { 0 };
    let era = y.div_euclid(400);
    let yoe = (y - era * 400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u64;
    Some(era * 146097 + doe as i64 - 719468)
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32, day as u32)
}

fn central_is_daylight(utc_secs: i64) -> bool {
    let year = civil_from_unix(utc_secs).year;
    let start = nth_weekday(year, 3, 0, 2).saturating_add(8 * 3600);
    let end = nth_weekday(year, 11, 0, 1).saturating_add(7 * 3600);
    utc_secs >= start && utc_secs < end
}

/// `weekday`: 0 = Sunday. `n` is 1-based.
fn nth_weekday(year: i32, month: u32, weekday: u32, n: u32) -> i64 {
    let first = days_from_civil(year, month, 1).unwrap_or(0);
    let first_weekday = (first + 4).rem_euclid(7) as u32; // 1970-01-01 was Thursday
    let delta = (weekday + 7 - first_weekday) % 7;
    let day = 1 + delta + (n - 1) * 7;
    days_from_civil(year, month, day).unwrap_or(first) * 86400
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_epoch_and_a_known_afternoon() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(parse_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(format_utc(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn chicago_labels_use_daylight_time_in_summer_only() {
        assert_eq!(
            format_chicago("2026-11-17T12:00:00Z"),
            "Nov 17, 2026 · 6:00 AM CT"
        );
        assert_eq!(
            format_chicago("2026-07-15T12:00:00Z"),
            "Jul 15, 2026 · 7:00 AM CT"
        );
    }
}
