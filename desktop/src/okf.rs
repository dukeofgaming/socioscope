use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    List(Vec<Value>),
    Map(Vec<(String, Value)>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust {
    Unverified,
    MachineConfirmed,
    HumanReviewed,
}

impl Trust {
    pub fn label(self) -> &'static str {
        match self {
            Trust::Unverified => "unverified",
            Trust::MachineConfirmed => "machine-confirmed",
            Trust::HumanReviewed => "human-reviewed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Concept {
    pub id: String,
    pub type_name: String,
    pub title: String,
    pub description: String,
    pub resource: String,
    pub tags: Vec<String>,
    pub status: String,
    pub trust: Trust,
    pub generated_by: String,
    pub verified_by: Vec<String>,
    pub body: String,
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub bytes: u64,
}

#[derive(Clone, Debug)]
pub struct Bundle {
    pub packaged: bool,
    pub version: String,
    pub title: String,
    pub description: String,
    pub license: String,
    pub log: String,
    pub concepts: Vec<Concept>,
    pub assets: Vec<Asset>,
}

impl Bundle {
    pub fn empty() -> Self {
        Self {
            packaged: false,
            version: String::new(),
            title: String::new(),
            description: String::new(),
            license: String::new(),
            log: String::new(),
            concepts: Vec::new(),
            assets: Vec::new(),
        }
    }
}

pub fn load(dir: &Path) -> Bundle {
    let mut bundle = Bundle::empty();
    walk(dir, Path::new(""), &mut bundle);
    bundle.concepts.sort_by(|a, b| a.id.cmp(&b.id));
    bundle.assets.sort_by(|a, b| a.name.cmp(&b.name));
    bundle
}

pub fn seed(dir: &Path, title: &str, description: &str) -> std::io::Result<()> {
    let title_yaml = yaml_quote(title);
    let description_yaml = yaml_quote(description);
    let caption_title = yaml_quote(&format!("{title} captions"));
    let session = format!(
        "---\ntype: Session\ntitle: {title_yaml}\ndescription: {description_yaml}\ntags: [session, captions]\nstatus: draft\nresource: ./captions.md\ngenerated: {{ by: \"process:socioscope\", at: \"2026-11-22T00:00:00Z\" }}\n---\n\nImported captions. The cue sheet is the SRT asset.\n\nSee the [captions](captions.md).\n"
    );
    let captions = format!(
        "---\ntype: Captions\ntitle: {caption_title}\ndescription: {description_yaml}\ntags: [captions, srt]\nstatus: draft\nresource: ./captions.srt\ngenerated: {{ by: \"process:socioscope\", at: \"2026-11-22T00:00:00Z\" }}\nsources:\n  - id: srt\n    resource: ./captions.srt\n    title: captions.srt\n---\n\nThe studio plays this SRT.\n"
    );
    let index = format!(
        "---\nokf_version: \"0.3\"\ntitle: {title_yaml}\ndescription: {description_yaml}\n---\n\n# Session\n\n* [Session](session.md) - {description}\n* [Captions](captions.md) - the SRT cue sheet\n"
    );
    let log = "# Directory Update Log\n\n## 2026-11-22\n* **Creation**: Imported captions into an OKF bundle.\n";
    fs::write(dir.join("index.md"), index)?;
    fs::write(dir.join("session.md"), session)?;
    fs::write(dir.join("captions.md"), captions)?;
    fs::write(dir.join("log.md"), log)?;
    Ok(())
}

pub fn parse_document(text: &str) -> (Vec<(String, Value)>, String) {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = text.trim_start_matches('\u{feff}');
    if let Some(rest) = trimmed.strip_prefix("---\n") {
        if let Some((front, body)) = rest.split_once("\n---") {
            let body = body.trim_start_matches('\n');
            return (parse_map_text(front), body.to_string());
        }
    }
    (parse_map_text(trimmed), String::new())
}

pub fn parse_map_text(text: &str) -> Vec<(String, Value)> {
    let lines = text
        .lines()
        .filter_map(|line| {
            let text = line.trim();
            if text.is_empty() || text.starts_with('#') {
                return None;
            }
            let indent = line.chars().take_while(|ch| *ch == ' ').count();
            Some(Line {
                indent,
                text: text.to_string(),
            })
        })
        .collect::<Vec<_>>();
    parse_properties(&lines, 0, 0).0
}

pub fn field<'a>(map: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    map.iter().find(|(name, _)| name == key).map(|(_, value)| value)
}

pub fn trust_tier(generated_at: Option<&str>, verified: &[(String, String)]) -> Trust {
    let generated = generated_at.map(normalize_time);
    let mut human = false;
    let mut machine = false;
    for (actor, at) in verified {
        let current = match &generated {
            None => true,
            Some(generated) => normalize_time(at) >= *generated,
        };
        if !current {
            continue;
        }
        if actor.starts_with("human:") {
            human = true;
        } else {
            machine = true;
        }
    }
    if human {
        Trust::HumanReviewed
    } else if machine {
        Trust::MachineConfirmed
    } else {
        Trust::Unverified
    }
}

fn walk(dir: &Path, rel: &Path, bundle: &mut Bundle) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            walk(&path, &rel.join(name.as_ref()), bundle);
            continue;
        }
        let relative = rel.join(name.as_ref());
        let relative_name = relative.to_string_lossy().replace('\\', "/");
        if name.ends_with(".md") {
            if name == "index.md" || name == "log.md" {
                if rel.as_os_str().is_empty() {
                    read_reserved(&path, name.as_ref(), bundle);
                }
                continue;
            }
            if let Some(concept) = read_concept(&path, &relative_name) {
                bundle.concepts.push(concept);
            }
            continue;
        }
        bundle.assets.push(Asset {
            name: relative_name,
            bytes: meta.len(),
        });
    }
}

fn read_reserved(path: &Path, name: &str, bundle: &mut Bundle) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let (map, body) = parse_document(&text);
    if name == "log.md" {
        bundle.log = body.trim().to_string();
        return;
    }
    bundle.packaged = true;
    bundle.version = text_field(&map, "okf_version");
    bundle.title = text_field(&map, "title");
    bundle.description = text_field(&map, "description");
    bundle.license = text_field(&map, "license");
}

fn read_concept(path: &Path, relative: &str) -> Option<Concept> {
    let text = fs::read_to_string(path).ok()?;
    let (map, body) = parse_document(&text);
    let type_name = text_field(&map, "type");
    if type_name.is_empty() {
        return None;
    }
    let id = relative.trim_end_matches(".md").to_string();
    let title = {
        let title = text_field(&map, "title");
        if title.is_empty() {
            PathBuf::from(&id)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&id)
                .to_string()
        } else {
            title
        }
    };
    let tags = field(&map, "tags")
        .and_then(|value| match value {
            Value::List(items) => Some(
                items
                    .iter()
                    .filter_map(|item| match item {
                        Value::Text(text) => Some(text.clone()),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    let generated = field(&map, "generated").and_then(|value| match value {
        Value::Map(pairs) => Some(pairs),
        _ => None,
    });
    let generated_by = generated
        .and_then(|pairs| field(pairs, "by"))
        .and_then(as_text)
        .unwrap_or_default()
        .to_string();
    let generated_at = generated.and_then(|pairs| field(pairs, "at")).and_then(as_text);
    let verified = verified_events(field(&map, "verified"));
    let verified_by = verified.iter().map(|(actor, _)| actor.clone()).collect();
    let mut status = text_field(&map, "status");
    if status.is_empty() {
        status = "stable".to_string();
    }
    Some(Concept {
        id,
        type_name,
        title,
        description: text_field(&map, "description"),
        resource: text_field(&map, "resource"),
        tags,
        status,
        trust: trust_tier(generated_at, &verified),
        generated_by,
        verified_by,
        body: body.trim().to_string(),
    })
}

fn verified_events(value: Option<&Value>) -> Vec<(String, String)> {
    let Some(value) = value else {
        return Vec::new();
    };
    let items = match value {
        Value::List(items) => items.clone(),
        Value::Map(_) => vec![value.clone()],
        Value::Text(_) => return Vec::new(),
    };
    items
        .into_iter()
        .filter_map(|item| match item {
            Value::Map(pairs) => {
                let by = field(&pairs, "by").and_then(as_text)?.to_string();
                let at = field(&pairs, "at").and_then(as_text)?.to_string();
                Some((by, at))
            }
            _ => None,
        })
        .collect()
}

fn text_field(map: &[(String, Value)], key: &str) -> String {
    field(map, key).and_then(as_text).unwrap_or("").to_string()
}

fn as_text(value: &Value) -> Option<&str> {
    match value {
        Value::Text(text) => Some(text.as_str()),
        _ => None,
    }
}

fn normalize_time(value: &str) -> String {
    let value = value.trim();
    if value.len() == 10 && value.as_bytes().get(4) == Some(&b'-') {
        format!("{value}T00:00:00Z")
    } else {
        value.to_string()
    }
}

struct Line {
    indent: usize,
    text: String,
}

fn parse_properties(lines: &[Line], mut index: usize, indent: usize) -> (Vec<(String, Value)>, usize) {
    let mut pairs = Vec::new();
    while index < lines.len() {
        if lines[index].indent < indent || lines[index].text.starts_with("- ") {
            break;
        }
        if lines[index].indent > indent {
            index += 1;
            continue;
        }
        let Some((key, raw)) = split_key(&lines[index].text) else {
            index += 1;
            continue;
        };
        index += 1;
        if raw.is_empty() {
            if index < lines.len() && lines[index].text.starts_with("- ") {
                let child = lines[index].indent;
                let (list, next) = parse_list(lines, index, child);
                pairs.push((key, Value::List(list)));
                index = next;
            } else if index < lines.len() && lines[index].indent > indent {
                let child = lines[index].indent;
                let (map, next) = parse_properties(lines, index, child);
                pairs.push((key, Value::Map(map)));
                index = next;
            } else {
                pairs.push((key, Value::Text(String::new())));
            }
        } else {
            pairs.push((key, parse_inline(&raw)));
        }
    }
    (pairs, index)
}

fn parse_list(lines: &[Line], mut index: usize, indent: usize) -> (Vec<Value>, usize) {
    let mut items = Vec::new();
    while index < lines.len() {
        if lines[index].indent < indent {
            break;
        }
        if lines[index].indent == indent && lines[index].text.starts_with("- ") {
            let rest = lines[index].text[2..].trim().to_string();
            index += 1;
            if let Some((key, raw)) = split_key(&rest) {
                let mut pairs = vec![(key, if raw.is_empty() { Value::Text(String::new()) } else { parse_inline(&raw) })];
                if index < lines.len() && lines[index].indent > indent && !lines[index].text.starts_with("- ") {
                    let child = lines[index].indent;
                    let (more, next) = parse_properties(lines, index, child);
                    pairs.extend(more);
                    index = next;
                }
                items.push(Value::Map(pairs));
            } else {
                items.push(parse_inline(&rest));
            }
        } else {
            break;
        }
    }
    (items, index)
}

fn split_key(text: &str) -> Option<(String, String)> {
    let mut quoted = false;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch == '"' {
            quoted = !quoted;
        }
        if ch == ':' && !quoted {
            let key = text[..index].trim().to_string();
            if key.is_empty() {
                return None;
            }
            let raw = text[index + 1..].trim().to_string();
            return Some((key, raw));
        }
        index += 1;
    }
    None
}

fn parse_inline(raw: &str) -> Value {
    let raw = raw.trim();
    if raw.starts_with('[') && raw.ends_with(']') {
        let inner = &raw[1..raw.len() - 1];
        let items = split_top(inner, ',')
            .into_iter()
            .filter(|item| !item.is_empty())
            .map(|item| parse_inline(item.trim()))
            .collect();
        return Value::List(items);
    }
    if raw.starts_with('{') && raw.ends_with('}') {
        let inner = &raw[1..raw.len() - 1];
        let pairs = split_top(inner, ',')
            .into_iter()
            .filter_map(|item| {
                let (key, value) = split_key(item.trim())?;
                Some((key, parse_inline(&value)))
            })
            .collect();
        return Value::Map(pairs);
    }
    Value::Text(unquote(raw))
}

fn split_top(text: &str, separator: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    let mut quoted = false;
    for (index, ch) in text.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            '[' | '{' if !quoted => depth += 1,
            ']' | '}' if !quoted && depth > 0 => depth -= 1,
            _ if ch == separator && depth == 0 && !quoted => {
                parts.push(text[start..index].trim().to_string());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim().to_string());
    parts
}

fn yaml_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn unquote(raw: &str) -> String {
    let raw = raw.trim();
    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        raw[1..raw.len() - 1].to_string()
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_frontmatter_trust_and_sources() {
        let text = r#"---
type: Recording
title: Fear Itself
description: First inaugural
tags: [session, audio]
status: stable
resource: ../../tests/data/fdr.mp3
generated: { by: "human:socioscope", at: "2026-11-22T00:00:00Z" }
verified: { by: "human:socioscope", at: "2026-11-22T00:00:00Z" }
sources:
  - id: audio
    resource: ../../tests/data/fdr.mp3
    title: FDR inaugural
    author: "human:roosevelt"
    last_modified: "1933-03-04"
---

See the [captions](captions.md).
"#;
        let (map, body) = parse_document(text);
        assert_eq!(text_field(&map, "type"), "Recording");
        assert_eq!(text_field(&map, "okf_version"), "");
        let tags = field(&map, "tags").unwrap();
        match tags {
            Value::List(items) => assert_eq!(items.len(), 2),
            other => panic!("tags {other:?}"),
        }
        let sources = field(&map, "sources").unwrap();
        match sources {
            Value::List(items) => match &items[0] {
                Value::Map(pairs) => assert_eq!(text_field(pairs, "author"), "human:roosevelt"),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        assert!(body.contains("captions.md"));
        let generated_at = Some("2026-11-22T00:00:00Z");
        assert_eq!(
            trust_tier(generated_at, &[("human:socioscope".into(), "2026-11-22T00:00:00Z".into())]),
            Trust::HumanReviewed
        );
        assert_eq!(
            trust_tier(generated_at, &[("process:socioscope".into(), "2026-11-22T01:00:00Z".into())]),
            Trust::MachineConfirmed
        );
        assert_eq!(
            trust_tier(generated_at, &[("human:socioscope".into(), "2026-11-21T00:00:00Z".into())]),
            Trust::Unverified
        );
    }

    #[test]
    fn loads_the_fear_itself_bundle() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../library/fear-itself");
        let bundle = load(&root);
        assert!(bundle.packaged);
        assert_eq!(bundle.version, "0.3");
        assert!(bundle.concepts.iter().any(|concept| concept.type_name == "Session"));
        assert!(bundle.concepts.iter().any(|concept| concept.type_name == "Captions"));
        assert!(bundle.assets.iter().any(|asset| asset.name == "captions.srt"));
        let session = bundle.concepts.iter().find(|concept| concept.type_name == "Session").unwrap();
        assert_eq!(session.trust, Trust::HumanReviewed);
    }

    #[test]
    fn sample_exchange_is_an_unverified_draft() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../library/exchange");
        let bundle = load(&root);
        assert_eq!(bundle.version, "0.3");
        let session = bundle.concepts.iter().find(|concept| concept.type_name == "Session").unwrap();
        assert_eq!(session.status, "draft");
        assert_eq!(session.trust, Trust::Unverified);
        assert!(bundle.concepts.iter().any(|concept| concept.type_name == "Speakers"));
    }
}
