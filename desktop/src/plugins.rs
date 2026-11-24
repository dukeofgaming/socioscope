use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::gguf::{self, ModelHeader};
use crate::library::{self, Recording};
use crate::srt::Cue;

const NOUL_MARK: f32 = 0.55;

#[derive(Clone, Debug)]
pub struct EngineCard {
    pub header: ModelHeader,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FallacyHit {
    pub cue_index: usize,
    pub start_ms: u64,
    pub speaker: String,
    pub quote: String,
    pub kind: String,
    pub label: String,
    pub noul: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ThemeScore {
    pub id: String,
    pub label: String,
    pub score: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProfileSession {
    pub session_id: String,
    pub title: String,
    pub themes: Vec<ThemeScore>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub speaker: String,
    pub slug: String,
    pub sessions: Vec<ProfileSession>,
    pub themes: Vec<ThemeScore>,
    pub correlation: String,
    pub laya: String,
    pub bonsai: String,
}

pub struct RunNote {
    pub summary: String,
}

pub trait Decider {
    fn decide(&self, request: &Value) -> Result<Value, String>;
}

pub fn laya_status(path: &str) -> String {
    match inspect(path, true) {
        Ok(card) => format!(
            "Laya loaded · {} · {} tensors · {:.1} MB",
            card.header.architecture,
            card.header.tensors,
            card.header.bytes as f64 / (1024.0 * 1024.0)
        ),
        Err(err) => err,
    }
}

pub fn bonsai_status(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return "Optional. A local Bonsai file or server correlates profiles.".into();
    }
    match inspect(path, false) {
        Ok(card) => {
            let hint = if gguf::is_bonsai(&card.header) {
                "Bonsai family"
            } else {
                "GGUF header loaded"
            };
            format!(
                "{hint} · {} · {} tensors · {:.1} MB",
                if card.header.architecture.is_empty() {
                    "unnamed"
                } else {
                    card.header.architecture.as_str()
                },
                card.header.tensors,
                card.header.bytes as f64 / (1024.0 * 1024.0)
            )
        }
        Err(err) => err,
    }
}

pub fn analyze_fallacies(
    recording: &Recording,
    enabled: bool,
    decider: &dyn Decider,
) -> Result<Vec<FallacyHit>, String> {
    if !enabled {
        return Err("Fallacies plugin is off.".into());
    }
    let mut hits = Vec::new();
    for (offset, cue) in recording.cues.iter().enumerate() {
        let request = fallacy_request(cue, recording.cues.get(offset.wrapping_sub(1)).filter(|_| offset > 0));
        let response = decider.decide(&request)?;
        if let Some(hit) = hit_from_answer(cue, &response) {
            hits.push(hit);
        }
    }
    write_fallacies(recording, &hits)?;
    Ok(hits)
}

pub fn analyze_fallacies_live(recording: &Recording, enabled: bool, model: &str, command: &str) -> Result<(RunNote, Vec<FallacyHit>), String> {
    if !enabled {
        return Err("Fallacies plugin is off.".into());
    }
    let card = inspect(model, true)?;
    let decider = LayaCommand {
        command: command.to_string(),
        model: card.path.clone(),
    };
    match decider.probe() {
        Ok(()) => {
            let hits = analyze_fallacies(recording, true, &decider)?;
            let summary = format!("{} fallacies · {}", hits.len(), laya_status(model));
            Ok((RunNote { summary }, hits))
        }
        Err(err) => Ok((
            RunNote {
                summary: format!("{}. {err}", laya_status(model)),
            },
            read_hits(&recording.dir),
        )),
    }
}

pub fn analyze_profiles(
    recordings: &[Recording],
    library_root: &Path,
    enabled: bool,
    decider: &dyn Decider,
    bonsai_endpoint: &str,
) -> Result<Vec<Profile>, String> {
    if !enabled {
        return Err("DSM-5 plugin is off.".into());
    }
    let mut touched = Vec::new();
    for recording in recordings {
        let speakers = speakers_of(recording);
        for (speaker, text) in speakers {
            let response = decider.decide(&profile_request(&speaker, &text))?;
            let themes = themes_from_answer(&response)?;
            let profile = merge_profile(library_root, &speaker, recording, themes, "");
            write_profile(library_root, &profile)?;
            write_session_pointer(recording, &profile)?;
            touched.push(profile.slug);
        }
    }
    let mut profiles = read_profiles_from(library_root);
    if !bonsai_endpoint.trim().is_empty() {
        for profile in profiles.iter_mut().filter(|profile| touched.contains(&profile.slug)) {
            let prompt = correlation_prompt(profile);
            match correlate(bonsai_endpoint.trim(), &prompt) {
                Ok(text) => {
                    profile.correlation = text;
                    profile.bonsai = "local server".into();
                    write_profile(library_root, profile)?;
                }
                Err(err) => {
                    profile.bonsai = err;
                    write_profile(library_root, profile)?;
                }
            }
        }
    }
    Ok(profiles)
}

pub fn analyze_profiles_live(
    recordings: &[Recording],
    enabled: bool,
    model: &str,
    command: &str,
    bonsai_path: &str,
    bonsai_endpoint: &str,
) -> Result<(RunNote, Vec<Profile>), String> {
    if !enabled {
        return Err("DSM-5 plugin is off.".into());
    }
    let card = inspect(model, true)?;
    let bonsai = match inspect(bonsai_path, false) {
        Ok(card) => format!("Bonsai header loaded ({})", card.header.architecture),
        Err(_) if bonsai_path.trim().is_empty() => "Bonsai file not set".into(),
        Err(err) => err,
    };
    let decider = LayaCommand {
        command: command.to_string(),
        model: card.path.clone(),
    };
    match decider.probe() {
        Ok(()) => {
            let profiles = analyze_profiles(recordings, &library::library_root(), true, &decider, bonsai_endpoint)?;
            Ok((
                RunNote {
                    summary: format!("{} profiles · {} · {bonsai}", profiles.len(), laya_status(model)),
                },
                profiles,
            ))
        }
        Err(err) => Ok((
            RunNote {
                summary: format!("{}. {err} {bonsai}", laya_status(model)),
            },
            read_profiles(),
        )),
    }
}

pub fn read_hits(dir: &Path) -> Vec<FallacyHit> {
    let path = dir.join("analyses").join("fallacies").join("findings.json");
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn read_profiles() -> Vec<Profile> {
    read_profiles_from(&library::library_root())
}

fn read_profiles_from(root: &Path) -> Vec<Profile> {
    let root = root.join("profiles");
    let Ok(entries) = fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut profiles = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path().join("profile.json");
        if let Ok(text) = fs::read_to_string(path) {
            if let Ok(profile) = serde_json::from_str::<Profile>(&text) {
                profiles.push(profile);
            }
        }
    }
    profiles.sort_by(|a, b| a.speaker.cmp(&b.speaker));
    profiles
}

pub fn fallacy_request(cue: &Cue, previous: Option<&Cue>) -> Value {
    let speaker = cue.speaker.clone().unwrap_or_else(|| "Unlabeled".into());
    let prior = previous
        .map(|cue| format!("Previous: {}", cue.plain()))
        .unwrap_or_else(|| "Previous: none".into());
    let mut criteria = serde_json::Map::new();
    for (id, detail) in KINDS {
        criteria.insert((*id).to_string(), json!(detail));
    }
    json!({
        "state": format!("Speaker: {speaker}\n{prior}\nTurn: {}", cue.plain()),
        "questions": {
            "holds": {
                "type": "noul",
                "instructions": "Does this turn depend on a logical fallacy?"
            },
            "kind": {
                "type": "choice",
                "instructions": "Which fallacy fits this turn?",
                "criteria": criteria
            }
        }
    })
}

pub fn profile_request(speaker: &str, text: &str) -> Value {
    let mut questions = serde_json::Map::new();
    for item in RUBRIC {
        questions.insert(
            item.id.to_string(),
            json!({
                "type": "score",
                "instructions": item.instructions,
                "criteria": item.levels,
            }),
        );
    }
    json!({
        "state": format!("Speaker: {speaker}\n{text}"),
        "questions": questions
    })
}

pub fn correlation_prompt(profile: &Profile) -> String {
    let themes = profile
        .themes
        .iter()
        .map(|theme| format!("{} {:.2}", theme.label, theme.score))
        .collect::<Vec<_>>()
        .join("; ");
    let sessions = profile
        .sessions
        .iter()
        .map(|session| session.title.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Speaker label: {}\nSessions: {}\nEvidence themes: {}\nRelate which themes rise together across these conversations, and which session each theme comes from. Do not name a disorder. Do not say the person meets diagnostic criteria.",
        profile.speaker, sessions, themes
    )
}

pub fn correlate(endpoint: &str, prompt: &str) -> Result<String, String> {
    let url = endpoint.trim().trim_end_matches('/');
    let (host, port, path_prefix) = parse_http(url)?;
    let body = serde_json::to_string(&json!({
        "model": "bonsai",
        "messages": [{"role": "user", "content": prompt}]
    }))
    .map_err(|err| err.to_string())?;
    let path = format!("{path_prefix}/v1/chat/completions");
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(format!("{host}:{port}")).map_err(|err| format!("Bonsai server: {err}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(60))).ok();
    stream.write_all(request.as_bytes()).map_err(|err| err.to_string())?;
    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(|err| err.to_string())?;
    let payload = response.split("\r\n\r\n").nth(1).unwrap_or(&response);
    correlation_text(payload)
}

fn correlation_text(payload: &str) -> Result<String, String> {
    let value: Value = serde_json::from_str(payload.trim()).map_err(|err| format!("Bonsai response: {err}"))?;
    value
        .pointer("/choices/0/message/content")
        .and_then(|content| content.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Bonsai response has no message".into())
}

struct LayaCommand {
    command: String,
    model: String,
}

impl LayaCommand {
    fn probe(&self) -> Result<(), String> {
        let output = Command::new(&self.command)
            .arg("--version")
            .output()
            .map_err(|_| format!("`{}` is not on PATH. The Laya file loaded. Run it locally with the laya binary.", self.command))?;
        if output.status.success() || !output.stdout.is_empty() || !output.stderr.is_empty() {
            Ok(())
        } else {
            Err(format!("`{}` did not start.", self.command))
        }
    }
}

impl Decider for LayaCommand {
    fn decide(&self, request: &Value) -> Result<Value, String> {
        let payload = serde_json::to_string(request).map_err(|err| err.to_string())?;
        let mut child = Command::new(&self.command)
            .args(["test", "--backend", "cpu", "-m", &self.model, "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|err| format!("could not start {}: {err}", self.command))?;
        child
            .stdin
            .take()
            .ok_or_else(|| "laya stdin closed".to_string())?
            .write_all(payload.as_bytes())
            .map_err(|err| err.to_string())?;
        let output = child.wait_with_output().map_err(|err| err.to_string())?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("laya exited {}: {}", output.status, err.trim()));
        }
        serde_json::from_slice(&output.stdout).map_err(|err| format!("laya output: {err}"))
    }
}

fn inspect(path: &str, require_laya: bool) -> Result<EngineCard, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("No model file yet".into());
    }
    let file = PathBuf::from(path);
    let file = if file.is_absolute() { file } else { library::repo_root().join(file) };
    if !file.is_file() {
        return Err(format!("File is not on disk ({})", file.display()));
    }
    let header = gguf::load_header(&file)?;
    if require_laya && !gguf::is_laya(&header) {
        return Err(format!(
            "This GGUF is '{}', not a Laya file",
            if header.architecture.is_empty() { "unnamed" } else { header.architecture.as_str() }
        ));
    }
    Ok(EngineCard {
        header,
        path: file.display().to_string(),
    })
}

fn hit_from_answer(cue: &Cue, response: &Value) -> Option<FallacyHit> {
    let answers = response.get("answers").unwrap_or(response);
    let noul = answers.pointer("/holds/noul").and_then(|value| value.as_f64()).unwrap_or(0.0) as f32;
    let kind = answers.pointer("/kind/choice").and_then(|value| value.as_str()).unwrap_or("none");
    if noul < NOUL_MARK || kind == "none" || kind.is_empty() {
        return None;
    }
    let label = KINDS
        .iter()
        .find(|(id, _)| *id == kind)
        .map(|(_, detail)| *detail)
        .unwrap_or(kind);
    Some(FallacyHit {
        cue_index: cue.index,
        start_ms: cue.start_ms,
        speaker: cue.speaker.clone().unwrap_or_else(|| "Unlabeled".into()),
        quote: cue.plain(),
        kind: kind.to_string(),
        label: label.to_string(),
        noul,
    })
}

fn themes_from_answer(response: &Value) -> Result<Vec<ThemeScore>, String> {
    let answers = response.get("answers").unwrap_or(response);
    let mut themes = Vec::new();
    for item in RUBRIC {
        let score = answers
            .get(item.id)
            .and_then(|answer| answer.get("score"))
            .and_then(|value| value.as_f64())
            .ok_or_else(|| format!("Laya answer is missing {}", item.id))? as f32;
        themes.push(ThemeScore {
            id: item.id.to_string(),
            label: item.label.to_string(),
            score,
        });
    }
    Ok(themes)
}

fn speakers_of(recording: &Recording) -> Vec<(String, String)> {
    let mut names = Vec::new();
    for cue in &recording.cues {
        let name = cue.speaker.clone().unwrap_or_else(|| "Unlabeled".into());
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
        .into_iter()
        .map(|name| {
            let text = recording
                .cues
                .iter()
                .filter(|cue| cue.speaker.clone().unwrap_or_else(|| "Unlabeled".into()) == name)
                .map(|cue| cue.plain())
                .collect::<Vec<_>>()
                .join("\n");
            let text = text.chars().take(1200).collect::<String>();
            (name, text)
        })
        .filter(|(_, text)| !text.trim().is_empty())
        .collect()
}

fn merge_profile(library_root: &Path, speaker: &str, recording: &Recording, themes: Vec<ThemeScore>, correlation: &str) -> Profile {
    let slug = slug(speaker);
    let mut profile = read_one(library_root, &slug).unwrap_or(Profile {
        speaker: speaker.to_string(),
        slug: slug.clone(),
        sessions: Vec::new(),
        themes: Vec::new(),
        correlation: String::new(),
        laya: "laya".into(),
        bonsai: String::new(),
    });
    profile.speaker = speaker.to_string();
    profile.sessions.retain(|session| session.session_id != recording.id);
    profile.sessions.push(ProfileSession {
        session_id: recording.id.clone(),
        title: recording.title.clone(),
        themes: themes.clone(),
    });
    profile.themes = mean_themes(&profile.sessions);
    if !correlation.is_empty() {
        profile.correlation = correlation.to_string();
    }
    profile.laya = "laya".into();
    profile
}

fn mean_themes(sessions: &[ProfileSession]) -> Vec<ThemeScore> {
    RUBRIC
        .iter()
        .map(|item| {
            let values = sessions
                .iter()
                .flat_map(|session| session.themes.iter())
                .filter(|theme| theme.id == item.id)
                .map(|theme| theme.score)
                .collect::<Vec<_>>();
            let score = if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<f32>() / values.len() as f32
            };
            ThemeScore {
                id: item.id.to_string(),
                label: item.label.to_string(),
                score,
            }
        })
        .collect()
}

fn write_fallacies(recording: &Recording, hits: &[FallacyHit]) -> Result<(), String> {
    let dir = recording.dir.join("analyses").join("fallacies");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let stamp = iso_now();
    let mut index = String::from("---\nokf_version: \"0.3\"\ntitle: Fallacies\ndescription: Laya readings of one session.\n---\n\n# Findings\n\n");
    for (number, hit) in hits.iter().enumerate() {
        let name = format!("finding-{:03}.md", number + 1);
        index.push_str(&format!("* [{label}]({name}) - {speaker}\n", label = hit.kind, name = name, speaker = hit.speaker));
        let body = format!(
            "---\ntype: Fallacy\ntitle: {title}\ndescription: {detail}\ntags: [fallacy, {tag}]\nstatus: draft\nresource: ../../captions.srt\ngenerated: {{ by: \"process:fallacies\", at: \"{stamp}\" }}\n---\n\n{quote}\n\nLaya choice `{kind}`, hold {noul:.2}. Draft reading of one turn.\n",
            title = yaml(&hit.kind),
            detail = yaml(&format!("{} · cue {}", hit.speaker, hit.cue_index)),
            tag = hit.kind.replace('_', "-"),
            quote = hit.quote,
            kind = hit.kind,
            noul = hit.noul,
        );
        fs::write(dir.join(&name), body).map_err(|err| err.to_string())?;
    }
    fs::write(dir.join("index.md"), index).map_err(|err| err.to_string())?;
    fs::write(
        dir.join("agent.md"),
        format!(
            "---\ntype: AgentBrief\ntitle: Fallacy brief\nstatus: draft\ngenerated: {{ by: \"process:fallacies\", at: \"{stamp}\" }}\n---\n\n## Question\nWhich turns in this session depend on a fallacy?\n\n## Open\nFindings are draft. A person sets verified.\n\n## Next\nCheck the quoted turn before treating the label as reviewed.\n"
        ),
    )
    .map_err(|err| err.to_string())?;
    fs::write(dir.join("log.md"), format!("# Directory Update Log\n\n## {stamp}\n* **Run**: Laya fallacy pass, {} findings.\n", hits.len())).map_err(|err| err.to_string())?;
    fs::write(dir.join("findings.json"), serde_json::to_string_pretty(hits).map_err(|err| err.to_string())? + "\n").map_err(|err| err.to_string())?;
    append_log(&recording.dir, &format!("* **Fallacies**: Laya pass, {} findings.", hits.len()));
    Ok(())
}

fn write_profile(library_root: &Path, profile: &Profile) -> Result<(), String> {
    let dir = library_root.join("profiles").join(&profile.slug);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let stamp = iso_now();
    let themes = profile
        .themes
        .iter()
        .map(|theme| format!("* {} {:.2}", theme.label, theme.score))
        .collect::<Vec<_>>()
        .join("\n");
    let sessions = profile
        .sessions
        .iter()
        .map(|session| format!("* {}", session.title))
        .collect::<Vec<_>>()
        .join("\n");
    let correlation = if profile.correlation.is_empty() {
        "No Bonsai correlation yet.".to_string()
    } else {
        profile.correlation.clone()
    };
    fs::write(
        dir.join("index.md"),
        format!(
            "---\nokf_version: \"0.3\"\ntitle: {title}\ndescription: Evidence profile across conversations.\n---\n\n# Profile\n\n* [Evidence](profile.md) - theme scores for this speaker label\n* [Brief](agent.md) - what a later pass may add\n",
            title = yaml(&profile.speaker)
        ),
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("profile.md"),
        format!(
            "---\ntype: EvidenceProfile\ntitle: {title}\ndescription: Across conversations. Not a diagnosis.\ntags: [profile, evidence]\nstatus: draft\ngenerated: {{ by: \"process:dsm\", at: \"{stamp}\" }}\n---\n\nEvidence profile for the caption label **{speaker}**. This is not a diagnosis, and it does not say that anyone meets diagnostic criteria.\n\n## Themes\n\n{themes}\n\n## Sessions\n\n{sessions}\n\n## Correlation\n\n{correlation}\n",
            title = yaml(&profile.speaker),
            speaker = profile.speaker,
        ),
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("agent.md"),
        format!(
            "---\ntype: AgentBrief\ntitle: Profile brief\nstatus: draft\ngenerated: {{ by: \"process:dsm\", at: \"{stamp}\" }}\n---\n\n## Question\nWhich evidence themes travel with this speaker label across sessions?\n\n## Open\nOne profile is a caption label, not a confirmed person. Notes stay draft.\n\n## Next\nA later agent may add evidence links. A person sets verified. Do not write a disorder name.\n"
        ),
    )
    .map_err(|err| err.to_string())?;
    fs::write(dir.join("profile.json"), serde_json::to_string_pretty(profile).map_err(|err| err.to_string())? + "\n").map_err(|err| err.to_string())?;
    Ok(())
}

fn write_session_pointer(recording: &Recording, profile: &Profile) -> Result<(), String> {
    let dir = recording.dir.join("analyses").join("dsm");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let stamp = iso_now();
    let name = format!("{}.md", profile.slug);
    let index_path = dir.join("index.md");
    if !index_path.exists() {
        fs::write(
            &index_path,
            "---\nokf_version: \"0.3\"\ntitle: DSM-5 evidence\ndescription: Pointers from this session into speaker profiles.\n---\n\n# Profiles\n\n",
        )
        .map_err(|err| err.to_string())?;
    }
    let line = format!("* [{speaker}]({name})\n", speaker = profile.speaker, name = name);
    let current = fs::read_to_string(&index_path).unwrap_or_default();
    if !current.contains(&line) {
        fs::write(&index_path, format!("{current}{line}")).map_err(|err| err.to_string())?;
    }
    fs::write(
        dir.join(&name),
        format!(
            "---\ntype: EvidenceLink\ntitle: {title}\ndescription: This session contributed to the profile.\nstatus: draft\nresource: ../../../profiles/{slug}/profile.md\ngenerated: {{ by: \"process:dsm\", at: \"{stamp}\" }}\n---\n\nSee the [profile](../../../profiles/{slug}/profile.md). The profile is evidence across conversations, not a diagnosis.\n",
            title = yaml(&profile.speaker),
            slug = profile.slug,
        ),
    )
    .map_err(|err| err.to_string())?;
    append_log(&recording.dir, &format!("* **DSM-5**: updated the evidence profile for {}.", profile.speaker));
    Ok(())
}

fn read_one(library_root: &Path, slug: &str) -> Option<Profile> {
    let text = fs::read_to_string(library_root.join("profiles").join(slug).join("profile.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn append_log(dir: &Path, line: &str) {
    let path = dir.join("log.md");
    if let Ok(mut file) = fs::OpenOptions::new().append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

fn parse_http(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "Bonsai endpoint must be an http:// URL".to_string())?;
    let (host_port, prefix) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = if let Some((host, port)) = host_port.split_once(':') {
        (host.to_string(), port.parse::<u16>().map_err(|_| "Bad port".to_string())?)
    } else {
        (host_port.to_string(), 80)
    };
    if host.is_empty() {
        return Err("Bonsai endpoint has no host".into());
    }
    let prefix = if prefix.is_empty() { String::new() } else { format!("/{prefix}") };
    Ok((host, port, prefix.trim_end_matches('/').to_string()))
}

fn slug(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_lowercase() } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() { "speaker".into() } else { slug }
}

fn yaml(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn iso_now() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let time = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        (time % 3600) / 60,
        time % 60
    )
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}

struct RubricItem {
    id: &'static str,
    label: &'static str,
    instructions: &'static str,
    levels: [&'static str; 3],
}

const RUBRIC: &[RubricItem] = &[
    RubricItem {
        id: "speech.pressured",
        label: "Pressured pace",
        instructions: "How pressed or crowded is the delivery in this speech?",
        levels: ["calm and spaced", "somewhat hurried", "crowded and hard to interrupt"],
    },
    RubricItem {
        id: "speech.absolute",
        label: "Absolute claims",
        instructions: "How often does the speech treat a claim as beyond question?",
        levels: ["hedged or specific", "some unqualified claims", "repeated claims with no alternative"],
    },
    RubricItem {
        id: "speech.derail",
        label: "Topic shift",
        instructions: "How far does the speech jump away from the matter at hand?",
        levels: ["stays with the topic", "occasional jumps", "repeated jumps with no return"],
    },
    RubricItem {
        id: "affect.low",
        label: "Low affect",
        instructions: "How much of the speech describes the self as hopeless or flattened?",
        levels: ["no such description", "a passing note", "the description dominates"],
    },
];

const KINDS: &[(&str, &str)] = &[
    ("none", "No fallacy. The turn states a point or a fact."),
    ("ad_hominem", "Attacks the person instead of the claim."),
    ("straw_man", "Replaces the other claim with a weaker one."),
    ("false_cause", "Treats a sequence as a cause."),
    ("appeal_to_authority", "Treats a title as the proof."),
    ("false_dilemma", "Allows only two options when more exist."),
    ("slippery_slope", "Jumps to a distant harm without the steps."),
    ("circular", "Uses the conclusion as its own support."),
    ("hasty_generalization", "Draws a wide rule from one or two cases."),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::okf::Bundle;
    use crate::srt::Cue;

    struct Scripted;

    impl Decider for Scripted {
        fn decide(&self, request: &Value) -> Result<Value, String> {
            let state = request["state"].as_str().unwrap_or("");
            if request["questions"].get("kind").is_some() {
                let (kind, noul) = if state.contains("fool") {
                    ("ad_hominem", 0.91)
                } else {
                    ("none", 0.12)
                };
                return Ok(json!({
                    "answers": {
                        "holds": {"type": "noul", "noul": noul},
                        "kind": {"type": "choice", "choice": kind}
                    }
                }));
            }
            let mut answers = serde_json::Map::new();
            for item in RUBRIC {
                let score = if state.contains("hopeless") && item.id == "affect.low" { 2.0 } else { 0.4 };
                answers.insert(item.id.to_string(), json!({"type": "score", "score": score}));
            }
            Ok(json!({"answers": answers}))
        }
    }

    fn cue(index: usize, speaker: &str, text: &str) -> Cue {
        Cue {
            index,
            start_ms: index as u64 * 1000,
            end_ms: index as u64 * 1000 + 800,
            speaker: Some(speaker.into()),
            text: text.into(),
            raw: text.into(),
        }
    }

    fn recording(dir: &Path, id: &str, cues: Vec<Cue>) -> Recording {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("log.md"), "# Directory Update Log\n").unwrap();
        Recording {
            id: id.into(),
            title: id.into(),
            detail: String::new(),
            source: None,
            srt_path: None,
            cues,
            duration_ms: 10_000,
            peaks: Vec::new(),
            sample: true,
            order: 0,
            from_pipeline: false,
            bundle: Bundle::empty(),
            dir: dir.to_path_buf(),
        }
    }

    #[test]
    fn fallacies_write_a_draft_and_stay_off_when_disabled() {
        let dir = std::env::temp_dir().join(format!("socioscope-fallacy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let recording = recording(
            &dir,
            "exchange",
            vec![
                cue(1, "Kennedy", "We should fund the schools."),
                cue(2, "Nixon", "You only say that because you are a fool."),
            ],
        );
        let err = analyze_fallacies(&recording, false, &Scripted).unwrap_err();
        assert!(err.contains("off"));
        let hits = analyze_fallacies(&recording, true, &Scripted).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "ad_hominem");
        let saved = read_hits(&dir);
        assert_eq!(saved, hits);
        let concept = fs::read_to_string(dir.join("analyses/fallacies/finding-001.md")).unwrap();
        assert!(concept.contains("type: Fallacy"));
        assert!(concept.contains("status: draft"));
        let request = fallacy_request(&recording.cues[1], Some(&recording.cues[0]));
        assert!(request["questions"]["kind"]["criteria"]["straw_man"].is_string());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn profiles_accumulate_across_sessions_and_the_prompt_stays_descriptive() {
        let root = std::env::temp_dir().join(format!("socioscope-profiles-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let library = root.join("library");
        fs::create_dir_all(&library).unwrap();
        let first = recording(
            &root.join("one"),
            "one",
            vec![cue(1, "Kennedy", "I feel hopeless about the whole thing.")],
        );
        let second = recording(
            &root.join("two"),
            "two",
            vec![cue(1, "Kennedy", "We can try another route.")],
        );
        let profiles = analyze_profiles(&[first, second], &library, true, &Scripted, "").unwrap();
        let kennedy = profiles.iter().find(|profile| profile.speaker == "Kennedy").unwrap();
        assert_eq!(kennedy.sessions.len(), 2);
        let low = kennedy.themes.iter().find(|theme| theme.id == "affect.low").unwrap();
        assert!(low.score > 1.0);
        let prompt = correlation_prompt(kennedy);
        assert!(prompt.contains("Do not name a disorder"));
        assert!(!prompt.to_lowercase().contains("diagnose"));
        let again = analyze_profiles(
            &[recording(&root.join("two"), "two", vec![cue(1, "Kennedy", "We can try another route.")])],
            &library,
            true,
            &Scripted,
            "",
        )
        .unwrap();
        let kennedy = again.iter().find(|profile| profile.speaker == "Kennedy").unwrap();
        assert_eq!(kennedy.sessions.len(), 2);
        let err = analyze_profiles(&[], &library, false, &Scripted, "").unwrap_err();
        assert!(err.contains("off"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bonsai_client_reads_a_chat_completion() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let body = br#"{"choices":[{"message":{"content":"Pressured pace shows up in one session and drops in the next."}}]}"#;
            let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });
        let text = correlate(&format!("http://127.0.0.1:{port}"), "Relate the themes.").unwrap();
        assert!(text.contains("Pressured pace"));
        assert!(correlation_text("{}").is_err());
    }
}
