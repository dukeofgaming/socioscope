/// Seconds to `HH:MM:SS,mmm`, using the same `timedelta` formatting as
/// `transcription.convert_to_timestamp`.
pub fn convert_to_timestamp(time_in_seconds: f64) -> String {
    let total_us = (time_in_seconds * 1_000_000.0).round() as i64;
    let days = total_us.div_euclid(86_400_000_000);
    let mut microseconds = total_us.rem_euclid(86_400_000_000);
    let hours = microseconds / 3_600_000_000;
    microseconds %= 3_600_000_000;
    let minutes = microseconds / 60_000_000;
    microseconds %= 60_000_000;
    let seconds = microseconds / 1_000_000;
    let micros = microseconds % 1_000_000;

    if days > 0 {
        let hours = hours + 24 * days;
        if micros == 0 {
            format!("{hours:02}:{minutes:02}:{seconds:02}")
        } else {
            format!("{hours:02}:{minutes:02}:{seconds:02}.{micros:06}")
        }
    } else {
        let seconds = seconds as f64 + (micros as f64 / 1_000_000.0);
        let rendered = format!("{seconds:06.3}").replace('.', ",");
        format!("{hours:02}:{minutes:02}:{rendered}")
    }
}

pub fn convert_to_milliseconds(time_in_seconds: f64) -> i64 {
    (time_in_seconds * 1000.0).trunc() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_match_python_timedelta_formatting() {
        let cases = [
            (0.0, "00:00:00,000"),
            (0.5, "00:00:00,500"),
            (1.0, "00:00:01,000"),
            (11.0, "00:00:11,000"),
            (65.25, "00:01:05,250"),
            (3600.25, "01:00:00,250"),
            (90000.5, "25:00:00.500000"),
        ];
        for (seconds, expected) in cases {
            assert_eq!(convert_to_timestamp(seconds), expected, "{seconds}");
        }
    }

    #[test]
    fn milliseconds_truncate_toward_zero() {
        assert_eq!(convert_to_milliseconds(0.5), 500);
        assert_eq!(convert_to_milliseconds(1.0), 1000);
        assert_eq!(convert_to_milliseconds(90000.5), 90_000_500);
    }
}
