use socioscope_web::router;

#[tokio::main]
async fn main() {
    let addr = std::env::var("SOCIOSCOPE_WEB_ADDR").unwrap_or_else(|_| "127.0.0.1:3210".into());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|error| panic!("bind {addr}: {error}"));
    let local = listener.local_addr().expect("local address");
    println!("socioscope web UI at http://{local}");
    axum::serve(listener, router()).await.expect("server");
}
