//! 零食仓库管理平台后端。
//!
//! 单体架构（契约 §0）：同一 Rust 进程同时托管 `/api/*` REST 接口与前端静态资源
//! （SPA fallback）。数据存储为 SQLite，默认 `data/app.db`。
//! 契约见仓库根目录 `CONTRACT.md`（前后端唯一事实来源）。

mod auth;
mod db;
mod error;
mod reminders;
mod snacks;
mod static_files;
mod users;
mod util;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use axum::routing::{delete, get, post, put};
use axum::{middleware, Router};
use rusqlite::Connection;

pub use error::ApiError;

/// 应用配置：数据库文件路径与静态资源目录。
#[derive(Clone, Debug)]
pub struct AppConfig {
    pub db_path: PathBuf,
    pub static_dir: PathBuf,
}

/// 全局共享状态：单个 SQLite 连接（互斥访问）与静态资源目录。
#[derive(Clone)]
pub struct AppState {
    db: Arc<Mutex<Connection>>,
    static_dir: PathBuf,
}

impl AppState {
    /// 获取数据库连接锁（仅在同步临界区内使用，不跨 `await` 持有）。
    pub(crate) fn conn(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// 构建应用：打开并初始化数据库（建表 + admin 幂等初始化），装配路由与认证中间件。
pub fn build_app(cfg: AppConfig) -> Result<Router, Box<dyn std::error::Error>> {
    let conn = db::open(&cfg.db_path)?;
    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
        static_dir: cfg.static_dir,
    };

    // `/api` 内层路由（认证/业务处理器）
    let api = Router::new()
        // 认证（契约 §3）
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/auth/change-password", post(auth::change_password))
        // 零食（契约 §3）
        .route("/snacks", get(snacks::list).post(snacks::create))
        .route("/snacks/{id}", put(snacks::update).delete(snacks::remove))
        .route("/locations", get(snacks::locations))
        // 过期提醒（契约 §3）
        .route("/reminders", get(reminders::list))
        // 账户管理（中间件强制 is_admin=1，否则 403）
        .route("/users", get(users::list).post(users::create))
        .route("/users/{id}", delete(users::remove))
        // 路径存在但方法不支持 → 统一 405 JSON
        .method_not_allowed_fallback(auth::api_method_not_allowed)
        // 未匹配的 /api/* → 统一 404 JSON（带有效 token 时）
        .fallback(auth::api_not_found);

    let app = Router::new()
        .nest("/api", api)
        // 非 /api 路径 → 静态资源 + SPA fallback（契约 §0）
        .fallback(static_files::spa_fallback)
        // 认证/授权中间件（契约 §2）
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .with_state(state);

    Ok(app)
}
