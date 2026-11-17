use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use socioscope_web::{router_with, Outcome};
use tower::ServiceExt;

async fn body(response: axum::response::Response) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn index_and_htmx_are_static() {
    let app = router_with(|_path| Outcome {
        code: 0,
        stderr: String::new(),
        output: String::new(),
    });

    let page = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let html = body(page).await;
    assert!(html.contains(r#"hx-post="/jobs""#), "{html}");
    assert!(html.contains(r#"src="/htmx.min.js""#), "{html}");

    let script = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/htmx.min.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(script.status(), StatusCode::OK);
    let script_body = body(script).await;
    assert!(script_body.contains("htmx"));
}

#[tokio::test]
async fn a_blank_path_is_rejected_without_a_job() {
    let app = router_with(|_path| panic!("transcriber should not run"));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/jobs")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("file=%20"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = body(response).await;
    assert!(html.contains("Enter a file or directory."), "{html}");
}

#[tokio::test]
async fn a_job_polls_until_the_transcript_is_ready() {
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let app = router_with(move |path| {
        let _ = release_rx.lock().unwrap().recv();
        Outcome {
            code: 0,
            stderr: String::new(),
            output: format!("srt for {path}"),
        }
    });

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/jobs")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("file=/tmp/sample.wav"))
                .unwrap(),
        )
        .await
        .unwrap();
    let created_html = body(created).await;
    assert!(
        created_html.contains(r#"hx-get="/jobs/1""#),
        "{created_html}"
    );
    assert!(created_html.contains("/tmp/sample.wav"), "{created_html}");

    release_tx.send(()).unwrap();

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut finished = String::new();
    while Instant::now() < deadline {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/jobs/1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        finished = body(response).await;
        if finished.contains("srt for /tmp/sample.wav") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished.contains("Finished"), "{finished}");
    assert!(finished.contains("srt for /tmp/sample.wav"), "{finished}");
    assert!(!finished.contains("hx-get"), "{finished}");
}

#[tokio::test]
async fn an_unknown_job_is_not_found() {
    let app = router_with(|_path| Outcome {
        code: 0,
        stderr: String::new(),
        output: String::new(),
    });
    let response = app
        .oneshot(
            Request::builder()
                .uri("/jobs/9")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
