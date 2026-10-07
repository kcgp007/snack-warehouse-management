//! 服务入口：读取环境变量，构建应用并监听端口。
//!
//! 环境变量（契约 §0/§1）：
//! - `SNACK_DB_PATH`：SQLite 文件路径，默认 `data/app.db`（相对当前工作目录）；
//! - `STATIC_DIR`：静态资源目录，默认编译期推导的 `backend/static`（与工作目录无关）；
//! - `ADMIN_USERNAME` / `ADMIN_PASSWORD`：admin 自动初始化的覆盖账号（默认 admin / admin123）；
//! - `HOST`（默认 `0.0.0.0`，监听所有网卡以便局域网远程访问）/ `PORT`（默认 `3000`）。

use std::path::PathBuf;

use snack_warehouse_backend::{build_app, AppConfig};

fn env_path(key: &str, default: PathBuf) -> PathBuf {
    std::env::var(key)
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or(default)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = env_path("SNACK_DB_PATH", PathBuf::from("data/app.db"));
    let static_dir = env_path(
        "STATIC_DIR",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
    );
    let host = std::env::var("HOST")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "0.0.0.0".to_owned());
    let port: u16 = std::env::var("PORT")
        .ok()
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse().ok())
        .unwrap_or(3000);

    let app = build_app(AppConfig {
        db_path: db_path.clone(),
        static_dir: static_dir.clone(),
    })?;

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    eprintln!("[server] listening on http://{addr}");
    eprintln!("[server] database: {}", db_path.display());
    eprintln!("[server] static dir: {}", static_dir.display());

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            eprintln!("[server] shutting down");
        })
        .await?;
    Ok(())
}
