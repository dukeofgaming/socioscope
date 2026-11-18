use std::io::Cursor;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use hound::{SampleFormat, WavSpec, WavWriter};
use http_body_util::BodyExt;
use socioscope_web::{router_from, AppConfig, FixedAnalyze};
use tower::ServiceExt;

fn wav(samples: usize) -> Vec<u8> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = WavWriter::new(&mut cursor, spec).unwrap();
        for index in 0..samples {
            let sample = ((index as f32 / 40.0).sin() * 8000.0) as i16;
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
    }
    cursor.into_inner()
}

fn app(dir: PathBuf) -> axum::Router {
    app_bin(dir, PathBuf::from("socioscope"))
}

fn app_bin(dir: PathBuf, bin: PathBuf) -> axum::Router {
    router_from(AppConfig {
        data_dir: dir.clone(),
        pace: 40.0,
        opening_wav: wav(8_000),
        opening_srt: "1\n00:00:00,000 --> 00:00:00,250\n[Ada Lovelace] hello from the tape\n\n2\n00:00:00,250 --> 00:00:00,500\n[Alan Turing] and another voice\n\n".into(),
        full_tape: None,
        analyzer: Box::new(FixedAnalyze {
            srt: "1\n00:00:00,000 --> 00:00:01,000\n[Grace Hopper] opened line\n\n".into(),
        }),
        socioscope_bin: bin,
        models_dir: dir.join("models"),
        config_path: dir.join("config.yaml"),
    })
}

async fn body(response: axum::response::Response) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn the_shell_is_a_responsive_app() {
    let dir = std::env::temp_dir().join(format!("socioscope-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let app = app(dir);
    let page = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let html = body(page).await;
    assert!(html.contains("socioscope"), "{html}");
    assert!(html.contains("Keep the take."), "{html}");
    assert!(html.contains("viewport-fit=cover"), "{html}");
    assert!(html.contains("Open Knowledge Format"), "{html}");
    assert!(html.contains("/record"), "{html}");
    assert!(html.contains("id=\"theme\""), "{html}");
    assert!(html.contains("Switch to dark mode"), "{html}");
    assert!(html.contains("grid-cols-4"), "{html}");
    assert!(html.contains("/models"), "{html}");

    let fragment = app
        .oneshot(
            Request::builder()
                .uri("/record")
                .header("hx-request", "true")
                .header("hx-target", "stage")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fragment.status(), StatusCode::OK);
    let partial = body(fragment).await;
    assert!(partial.contains("The debate, at 1×."), "{partial}");
    assert!(!partial.contains("<html"), "stage swaps must stay fragments:\n{partial}");
}

#[tokio::test]
async fn a_recording_becomes_an_okf_package() {
    let dir = std::env::temp_dir().join(format!("socioscope-rec-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let app = app(dir.clone());

    let started = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/recordings")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("source=opening"))
                .unwrap(),
        )
        .await
        .unwrap();
    let location = started
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let started_html = if started.status() == StatusCode::SEE_OTHER {
        String::new()
    } else {
        assert_eq!(started.status(), StatusCode::OK);
        body(started).await
    };

    let id = if let Some(id) = location.strip_prefix("/bundles/") {
        id.to_string()
    } else {
        let marker = "/recordings/";
        let start = started_html.find(marker).expect(&started_html) + marker.len();
        let end = started_html[start..]
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
            .map(|offset| start + offset)
            .unwrap_or(started_html.len());
        let id = started_html[start..end].to_string();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut found = String::new();
        while Instant::now() < deadline {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/recordings/{id}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            if response.status() == StatusCode::SEE_OTHER {
                found = response
                    .headers()
                    .get("location")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string();
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(found.ends_with(&id), "{found}");
        id
    };

    let bundle = dir.join("bundles").join(&id);
    let index = std::fs::read_to_string(bundle.join("index.md")).unwrap();
    assert!(index.starts_with("---\nokf_version: \"0.2\"\n---"), "{index}");
    let session = std::fs::read_to_string(bundle.join("session.md")).unwrap();
    assert!(session.contains("type: Recording Session"), "{session}");
    assert!(session.contains("capture: emulated"), "{session}");
    let transcript = std::fs::read_to_string(bundle.join("transcript.md")).unwrap();
    assert!(transcript.contains("type: Diarized Transcript"), "{transcript}");
    assert!(transcript.contains("hello from the tape"), "{transcript}");
    assert!(transcript.contains("and another voice"), "{transcript}");
    let wav = std::fs::read(bundle.join("references").join("recording.wav")).unwrap();
    assert!(wav.starts_with(b"RIFF"), "original take was not stored");
    let log = std::fs::read_to_string(bundle.join("log.md")).unwrap();
    assert!(log.contains("**Creation**"), "{log}");
    assert!(log.contains("**Update**"), "{log}");
    for note in ["index.md", "log.md"] {
        let text = std::fs::read_to_string(bundle.join(note)).unwrap();
        if note == "log.md" {
            assert!(!text.starts_with("---"), "{note}");
        }
    }
    let speaker = std::fs::read_to_string(
        bundle
            .join("speakers")
            .join("ada-lovelace.md"),
    )
    .unwrap();
    assert!(speaker.contains("type: Speaker"), "{speaker}");

    let concept = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/bundles/{id}/concepts/session.md"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let concept_html = body(concept).await;
    assert!(concept_html.contains("Recording Session"), "{concept_html}");
    assert!(concept_html.contains("OKF source"), "{concept_html}");

    let escaped = app
        .oneshot(
            Request::builder()
                .uri(format!("/bundles/{id}/concepts/../../secret.md"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(escaped.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_opened_file_is_stored_and_analyzed() {
    let dir = std::env::temp_dir().join(format!("socioscope-open-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("outside.wav");
    std::fs::write(&source, wav(1600)).unwrap();
    let app = app(dir.clone());

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/imports/path")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("path={}", source.display())))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let html = body(created).await;
    assert!(html.contains("outside") || html.contains("Opened"), "{html}");

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut transcript = String::new();
    while Instant::now() < deadline {
        if let Some(bundle) = std::fs::read_dir(dir.join("bundles"))
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .next()
        {
            let path = bundle.join("transcript.md");
            if path.is_file() {
                transcript = std::fs::read_to_string(&path).unwrap();
                if transcript.contains("opened line") {
                    let original = std::fs::read(bundle.join("references").join("original.wav")).unwrap();
                    assert_eq!(original, wav(1600));
                    let session = std::fs::read_to_string(bundle.join("session.md")).unwrap();
                    assert!(session.contains("capture: opened"), "{session}");
                    assert!(session.contains("analysis: done"), "{session}");
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(transcript.contains("opened line"), "{transcript}");
    assert!(transcript.contains("type: Diarized Transcript"), "{transcript}");
}

#[tokio::test]
async fn a_text_file_is_rejected() {
    let dir = std::env::temp_dir().join(format!("socioscope-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("notes.txt");
    std::fs::write(&source, b"hello").unwrap();
    let app = app(dir.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/imports/path")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("path={}", source.display())))
                .unwrap(),
        )
        .await
        .unwrap();
    let html = body(response).await;
    assert!(
        html.contains("The file must be an audio file: .wav, .m4a, .mp3"),
        "{html}"
    );
    assert!(!dir.join("bundles").exists() || std::fs::read_dir(dir.join("bundles")).unwrap().next().is_none());
}

#[tokio::test]
async fn the_model_shelf_is_the_cli_shelf() {
    let dir = std::env::temp_dir().join(format!("socioscope-shelf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("fake-socioscope");
    std::fs::write(
        &bin,
        r#"#!/bin/sh
if [ "$1" = "models" ] && [ "$2" = "list" ]; then
  printf '%s\n' '{"note":"QVAC is not the downloader","models":[{"id":"cactus-whistle","name":"Whistle","role":"transcription","badge":"Smallest","runtime":"Cactus Needle","runnable":false,"summary":"A tiny speech model.","license":"see card","released":"2026","size":"16.9 MB","state":"missing","active":false,"received":0,"total":1,"detail":""},{"id":"whisper-tiny-en","name":"Tiny","role":"transcription","badge":"Fast","runtime":"whisper.cpp","runnable":true,"summary":"Runs here.","license":"MIT","released":"2022","size":"74.0 MB","state":"ready","active":false,"received":1,"total":1,"detail":""}]}'
  exit 0
fi
exit 1
"#,
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&bin, perms).unwrap();
    let app = app_bin(dir, bin);

    let page = app
        .clone()
        .oneshot(Request::builder().uri("/models").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let html = body(page).await;
    assert!(html.contains("Whistle"), "{html}");
    assert!(html.contains("QVAC is not the downloader"), "{html}");
    assert!(html.contains("Download"), "{html}");
    assert!(html.contains(">Use<"), "{html}");
    assert!(html.contains("grid-cols-4"), "{html}");

    let fragment = app
        .oneshot(
            Request::builder()
                .uri("/models")
                .header("hx-request", "true")
                .header("hx-target", "stage")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let partial = body(fragment).await;
    assert!(partial.contains("Whistle"), "{partial}");
    assert!(!partial.contains("<html"), "{partial}");
}
