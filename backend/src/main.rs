// ============================================================
// 忆涟千言 Backend — standalone binary entry point
// ============================================================

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let addr = std::env::var("YILIAN_HOST").unwrap_or_else(|_| "127.0.0.1:9420".to_string());
    yilian_backend::serve(&addr).await;
}
