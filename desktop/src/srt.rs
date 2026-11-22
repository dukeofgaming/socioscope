#[derive(Clone, Debug)]
pub struct Cue {
    pub index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: Option<String>,
    pub text: String,
    pub raw: String,
}

#[derive(Clone, Debug)]
pub struct Assessment {
    pub cps: f32,
    pub chars: usize,
    pub lines: usize,
    pub max_line: usize,
    pub fast: bool,
    pub short: bool,
    pub long_line: bool,
    pub crowded: bool,
    pub overlap_next_ms: u64,
    pub overlap_prev_ms: u64,
    pub gap_before_ms: Option<u64>,
    pub gap_after_ms: Option<u64>,
}

pub const CPS_LIMIT: f32 = 21.0;
pub const LINE_LIMIT: usize = 42;
pub const MIN_VISIBLE_MS: u64 = 1000;

impl Cue {
    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }

    pub fn plain(&self) -> String {
        self.text.replace('\n', " ")
    }
}

pub fn parse_srt(input: &str) -> Vec<Cue> {
    let input = input
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut cues = Vec::new();
    for block in input.split("\n\n") {
        let lines: Vec<&str> = block
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        let Some(time_at) = lines.iter().position(|line| line.contains("-->")) else {
            continue;
        };
        let Some((start_ms, end_ms)) = parse_range(lines[time_at]) else {
            continue;
        };
        if end_ms <= start_ms || time_at + 1 >= lines.len() {
            continue;
        }
        let raw = lines[time_at + 1..].join("\n");
        let (speaker, text) = split_speaker(&raw);
        if text.trim().is_empty() {
            continue;
        }
        let index = lines
            .first()
            .and_then(|line| line.parse().ok())
            .unwrap_or(cues.len() + 1);
        cues.push(Cue {
            index,
            start_ms,
            end_ms,
            speaker,
            text,
            raw,
        });
    }
    cues
}

pub fn assess(cues: &[Cue], index: usize) -> Assessment {
    let cue = &cues[index];
    let lines: Vec<&str> = cue.text.lines().collect();
    let chars = cue.text.chars().filter(|ch| *ch != '\n').count();
    let seconds = cue.duration_ms() as f32 / 1000.0;
    let cps = if seconds > 0.0 {
        chars as f32 / seconds
    } else {
        0.0
    };
    let max_line = lines.iter().map(|line| line.chars().count()).max().unwrap_or(0);
    let overlap_next_ms = cues.get(index + 1).map_or(0, |next| {
        if next.start_ms < cue.end_ms {
            cue.end_ms - next.start_ms
        } else {
            0
        }
    });
    let overlap_prev_ms = if index > 0 && cue.start_ms < cues[index - 1].end_ms {
        cues[index - 1].end_ms - cue.start_ms
    } else {
        0
    };
    let gap_before_ms = if index > 0 && cues[index - 1].end_ms <= cue.start_ms {
        Some(cue.start_ms - cues[index - 1].end_ms)
    } else {
        None
    };
    let gap_after_ms = cues.get(index + 1).and_then(|next| {
        if cue.end_ms <= next.start_ms {
            Some(next.start_ms - cue.end_ms)
        } else {
            None
        }
    });
    Assessment {
        cps,
        chars,
        lines: lines.len(),
        max_line,
        fast: cps > CPS_LIMIT,
        short: cue.duration_ms() < MIN_VISIBLE_MS,
        long_line: max_line > LINE_LIMIT,
        crowded: lines.len() > 2,
        overlap_next_ms,
        overlap_prev_ms,
        gap_before_ms,
        gap_after_ms,
    }
}

pub fn warnings(cues: &[Cue], index: usize) -> Vec<String> {
    let note = assess(cues, index);
    let mut out = Vec::new();
    if note.fast {
        out.push(format!(
            "Reading speed is {:.0} characters per second. Broadcast captions usually stay under 21.",
            note.cps
        ));
    }
    if note.long_line {
        out.push(format!(
            "A line runs {} characters. The usual limit is 42.",
            note.max_line
        ));
    }
    if note.crowded {
        out.push(format!(
            "This cue has {} lines. Captions usually stop at two.",
            note.lines
        ));
    }
    if note.short {
        out.push(format!(
            "On screen for {:.1}s. Under a second is hard to read.",
            cues[index].duration_ms() as f32 / 1000.0
        ));
    }
    if note.overlap_next_ms > 40 {
        out.push(format!(
            "Overlaps the next cue by {} ms.",
            note.overlap_next_ms
        ));
    } else if note.overlap_prev_ms > 40 {
        out.push(format!(
            "Overlaps the previous cue by {} ms.",
            note.overlap_prev_ms
        ));
    }
    out
}

pub fn review_counts(cues: &[Cue]) -> (usize, usize, usize) {
    let mut fast = 0;
    let mut short = 0;
    let mut overlap = 0;
    for index in 0..cues.len() {
        let note = assess(cues, index);
        if note.fast || note.long_line || note.crowded {
            fast += 1;
        }
        if note.short {
            short += 1;
        }
        if note.overlap_next_ms > 40 {
            overlap += 1;
        }
    }
    (fast, short, overlap)
}

pub fn lanes(cues: &[Cue]) -> Vec<usize> {
    let mut ends: Vec<u64> = Vec::new();
    let mut assigned = Vec::with_capacity(cues.len());
    for cue in cues {
        if let Some(lane) = ends.iter().position(|end| *end <= cue.start_ms) {
            ends[lane] = cue.end_ms;
            assigned.push(lane);
        } else {
            ends.push(cue.end_ms);
            assigned.push(ends.len() - 1);
        }
    }
    assigned
}

pub fn srt_stamp(ms: u64) -> String {
    let hours = ms / 3_600_000;
    let minutes = (ms % 3_600_000) / 60_000;
    let seconds = (ms % 60_000) / 1000;
    let millis = ms % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

pub fn clock(ms: u64) -> String {
    let total = ms / 1000;
    let minutes = total / 60;
    let seconds = total % 60;
    if minutes >= 60 {
        format!("{}:{:02}:{:02}", minutes / 60, minutes % 60, seconds)
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

pub fn transport_clock(ms: f64) -> String {
    let ms = ms.max(0.0) as u64;
    let minutes = ms / 60_000;
    let seconds = (ms % 60_000) / 1000;
    let centis = (ms % 1000) / 10;
    format!("{minutes:02}:{seconds:02}.{centis:02}")
}

fn parse_range(line: &str) -> Option<(u64, u64)> {
    let (left, right) = line.split_once("-->")?;
    let start = parse_stamp(left.split_whitespace().next()?)?;
    let end = parse_stamp(right.split_whitespace().next()?)?;
    Some((start, end))
}

fn parse_stamp(stamp: &str) -> Option<u64> {
    let (clock, fraction) = stamp.split_once([',', '.']).unwrap_or((stamp, "0"));
    let mut parts = clock.split(':');
    let hours: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds: u64 = parts.next()?.parse().ok()?;
    let mut millis: u64 = fraction.chars().take(3).collect::<String>().parse().ok()?;
    if fraction.len() < 3 {
        millis *= 10u64.pow(3 - fraction.len() as u32);
    }
    Some(((hours * 60 + minutes) * 60 + seconds) * 1000 + millis)
}

fn split_speaker(raw: &str) -> (Option<String>, String) {
    let mut lines = raw.lines();
    let Some(first) = lines.next() else {
        return (None, String::new());
    };
    if let Some(rest) = first.strip_prefix('[') {
        if let Some((name, after)) = rest.split_once(']') {
            let name = name.trim();
            if !name.is_empty() && name.len() < 48 && !name.contains('\n') {
                let mut text = after.trim().to_string();
                for line in lines {
                    text.push('\n');
                    text.push_str(line.trim());
                }
                return (Some(name.to_string()), text);
            }
        }
    }
    (None, raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
1\n\
00:00:01,000 --> 00:00:04,200\n\
[Kennedy] And so, my fellow Americans,\n\
\n\
2\n\
00:00:03,900 --> 00:00:05,200\n\
[Kennedy] ask not what your country can do for you ask not what your country can do\n\
\n\
3\n\
00:00:05,500 --> 00:00:06,100\n\
[Nixon] Too short.\n";

    #[test]
    fn parses_speakers_and_stamps() {
        let cues = parse_srt(SAMPLE);
        assert_eq!(cues.len(), 3);
        assert_eq!(cues[0].start_ms, 1000);
        assert_eq!(cues[0].end_ms, 4200);
        assert_eq!(cues[0].speaker.as_deref(), Some("Kennedy"));
        assert_eq!(cues[0].text, "And so, my fellow Americans,");
        assert_eq!(srt_stamp(4200), "00:00:04,200");
    }

    #[test]
    fn flags_speed_overlap_and_short_cues() {
        let cues = parse_srt(SAMPLE);
        let first = assess(&cues, 0);
        assert!(first.overlap_next_ms > 40);
        assert!(!warnings(&cues, 0).is_empty());
        let fast = assess(&cues, 1);
        assert!(fast.fast);
        assert!(fast.long_line);
        let short = assess(&cues, 2);
        assert!(short.short);
        let lanes = lanes(&cues);
        assert_eq!(lanes[0], 0);
        assert_eq!(lanes[1], 1);
    }
}
