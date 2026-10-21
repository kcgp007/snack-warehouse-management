//! 服务入口：读取运行目录下的 `snack-warehouse.toml` 配置文件，构建应用并监听端口。
//!
//! 配置文件（优先）：运行目录下的 `snack-warehouse.toml`（与二进制同级）。
//! 配置项：`host`（默认 `0.0.0.0`）、`port`（默认 `3000`）、
//! `admin_username`（默认 `admin`）、`admin_password`（默认 `admin123`）。
//! 未设置的项使用默认值；不再读取 `/etc/snack-warehouse.env` 环境变量文件。
//!
//! 服务以 HTTPS 启动：证书与私钥固定从当前工作目录的 `data/certs/server.crt`、
//! `data/certs/server.key` 读取；两者都不存在时自动生成自签名证书（浏览器会提示不受信任）。

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::middleware;
use serde::Deserialize;
use snack_warehouse_backend::{build_app, tls, AppConfig};

const CONFIG_FILE: &str = "snack-warehouse.toml";

#[derive(Clone, Debug, Deserialize, Default)]
pub struct ServerConfig {
    // Flat TOML format (no nested [section] tables needed — keys are direct)
    pub host: Option<String>,
    pub port: Option<u16>,
    pub admin_username: Option<String>,
    pub admin_password: Option<String>,
}

fn load_server_config() -> ServerConfig {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let config_path = exe_dir.join(CONFIG_FILE);

    match std::fs::read_to_string(&config_path) {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => ServerConfig::default(),
    }
}

fn env_path(key: &str, default: PathBuf) -> PathBuf {
    std::env::var(key)
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or(default)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = load_server_config();

    let db_path = env_path("SNACK_DB_PATH", PathBuf::from("data/app.db"));
    let static_dir = env_path("STATIC_DIR", PathBuf::from("static"));
    let host = cfg.host.unwrap_or_else(|| "0.0.0.0".to_owned());
    let port = cfg.port.unwrap_or(3000);
    let admin_username = cfg.admin_username.unwrap_or_else(|| "admin".to_owned());
    let admin_password = cfg.admin_password.unwrap_or_else(|| "admin123".to_owned());

    let app = build_app(AppConfig {
        db_path: db_path.clone(),
        static_dir: static_dir.clone(),
        admin_username,
        admin_password,
    })?
    .layer(middleware::from_fn_with_state(
        tls::HttpsPort(port),
        tls::redirect_plain_http,
    ));

    let addr = socket_addr(&host, port)?;
    let tls = tls::load_or_generate().await?;

    eprintln!("[server] listening on https://{addr}");
    eprintln!("[server] database: {}", db_path.display());
    eprintln!("[server] static dir: {}", static_dir.display());
    eprintln!(
        "[server] TLS certificate: {} / {}",
        tls.cert_path.display(),
        tls.key_path.display()
    );
    eprintln!("[server] 同端口自动识别 HTTP 请求并跳转到 HTTPS");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let listener = tls::HybridListener::new(listener, tls.config);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<tls::HybridAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

/// 等待 Ctrl-C 或 SIGTERM（systemd stop / run.sh 转发）。
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            eprintln!("[server] shutting down");
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
    eprintln!("[server] shutting down");
}

/// 把 `HOST`/`PORT` 解析为监听地址；裸 IPv6 需要补方括号。
fn socket_addr(host: &str, port: u16) -> Result<SocketAddr, std::net::AddrParseError> {
    let text = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    text.parse()
}

#[cfg(test)]
mod tests {
    use super::socket_addr;

    #[test]
    fn parses_ipv4_and_ipv6_hosts() {
        assert_eq!(
            socket_addr("0.0.0.0", 3000).unwrap().to_string(),
            "0.0.0.0:3000"
        );
        assert_eq!(
            socket_addr("127.0.0.1", 8443).unwrap().to_string(),
            "127.0.0.1:8443"
        );
        assert_eq!(socket_addr("::", 3000).unwrap().to_string(), "[::]:3000");
        assert_eq!(
            socket_addr("[::1]", 3000).unwrap().to_string(),
            "[::1]:3000"
        );
        assert!(socket_addr("not-an-ip", 3000).is_err());
    }
}
