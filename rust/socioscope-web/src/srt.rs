//! Speaker-labelled SRT, the same shape the CLI writes.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub speakers: Vec<String>,
    pub text: String,
}

pub fn parse(text: &str) -> Vec<Cue> {
    let mut cues = Vec::new();
    let mut lines = text.lines().map(|line| line.trim_end()).peekable();
    while lines.peek().is_some() {
        while matches!(lines.peek(), Some(line) if line.trim().is_empty()) {
            lines.next();
        }
        if lines.peek().is_none() {
            break;
        }
        let _index = lines.next();
        let Some(times) = lines.next() else { break };
        let Some((start, end)) = split_times(times) else {
            continue;
        };
        let mut text_lines = Vec::new();
        while matches!(lines.peek(), Some(line) if !line.trim().is_empty()) {
            text_lines.push(lines.next().unwrap().trim());
        }
        let (speakers, spoken) = split_speakers(&text_lines.join(" "));
        if spoken.is_empty() && speakers.is_empty() {
            continue;
        }
        cues.push(Cue {
            start_ms: start,
            end_ms: end,
            speakers,
            text: spoken,
        });
    }
    cues
}

pub fn write(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (index, cue) in cues.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format!("{}\n", index + 1));
        out.push_str(&format!("{} --> {}\n", stamp(cue.start_ms), stamp(cue.end_ms)));
        if cue.speakers.is_empty() {
            out.push_str(&cue.text);
        } else {
            out.push('[');
            out.push_str(&cue.speakers.join(", "));
            out.push_str("] ");
            out.push_str(&cue.text);
        }
        out.push_str("\n");
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

pub fn visible(cues: &[Cue], elapsed_ms: u64) -> Vec<Cue> {
    cues.iter()
        .filter(|cue| cue.start_ms <= elapsed_ms)
        .cloned()
        .collect()
}

pub fn clock(ms: u64) -> String {
    let total = ms / 1000;
    format!("{}:{:02}", total / 60, total % 60)
}

fn stamp(ms: u64) -> String {
    let hours = ms / 3_600_000;
    let minutes = (ms / 60_000) % 60;
    let seconds = (ms / 1000) % 60;
    let millis = ms % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

fn split_times(line: &str) -> Option<(u64, u64)> {
    let (start, end) = line.split_once("-->")?;
    Some((parse_stamp(start.trim())?, parse_stamp(end.trim())?))
}

fn parse_stamp(text: &str) -> Option<u64> {
    let text = text.replace('.', ",");
    let (hms, millis) = text.split_once(',')?;
    let mut parts = hms.split(':');
    let hours: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds: u64 = parts.next()?.parse().ok()?;
    let millis: u64 = millis.parse().ok()?;
    Some(((hours * 60 + minutes) * 60 + seconds) * 1000 + millis)
}

fn split_speakers(text: &str) -> (Vec<String>, String) {
    let trimmed = text.trim();
    let Some(rest) = trimmed.strip_prefix('[') else {
        return (Vec::new(), trimmed.to_string());
    };
    let Some(end) = rest.find(']') else {
        return (Vec::new(), trimmed.to_string());
    };
    let speakers = rest[..end]
        .split(',')
        .map(str::trim)
        .filter(|speaker| !speaker.is_empty())
        .map(str::to_string)
        .collect();
    (speakers, rest[end + 1..].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_speaker_labels_and_hides_future_cues() {
        let cues = parse(
            "1\n00:00:00,000 --> 00:00:01,500\n[Ada, Bea] hello\n\n2\n00:00:02,000 --> 00:00:03,000\n[Bea] later\n",
        );
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].speakers, vec!["Ada".to_string(), "Bea".to_string()]);
        assert_eq!(cues[0].text, "hello");
        assert_eq!(visible(&cues, 1_000).len(), 1);
        assert_eq!(visible(&cues, 2_000).len(), 2);
        assert_eq!(clock(75_000), "1:15");
    }
}
