//! 静态资源托管与 SPA fallback（契约 §0）。
//!
//! - 非 `/api` 且磁盘上存在对应文件 → 直接返回；
//! - 文件不存在 → 回退 `index.html`（SPA fallback）；
//! - `index.html` 也不存在（`backend/static` 可能尚未由 build.sh 生成）→ 404 文本，绝不 panic；
//! - `/api/*` 永远优先于静态路由（这里再兜一层 JSON 404）。

use std::path::Path;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;
use crate::util::is_api_path;
use crate::AppState;

/// 静态资源 fallback 处理器（非 `/api` 路径）。
pub(crate) async fn spa_fallback(State(state): State<AppState>, req: Request) -> Response {
    let path = req.uri().path().to_owned();
    if is_api_path(&path) {
        return ApiError::not_found("接口不存在").into_response();
    }
    serve(&state.static_dir, &path).await
}

async fn serve(static_dir: &Path, path: &str) -> Response {
    // 只保留相对路径：去掉全部前导 `/`，拒绝 `..` 段（防目录穿越）。
    let rel = path.trim_start_matches('/');
    let safe = !rel.is_empty() && !rel.split('/').any(|segment| segment == "..");
    if safe {
        let candidate = static_dir.join(rel);
        if let Ok(bytes) = tokio::fs::read(&candidate).await {
            return file_response(rel, bytes);
        }
    }
    // SPA fallback → index.html
    match tokio::fs::read(static_dir.join("index.html")).await {
        Ok(index) => file_response("index.html", index),
        Err(_) => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

fn file_response(path: &str, bytes: Vec<u8>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type(path))
        .body(Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// 按扩展名推断 Content-Type（前端构建产物所需的常见类型）。
fn content_type(path: &str) -> &'static str {
    let ext = path
        .rsplit('.')
        .next()
        .filter(|ext| !ext.contains('/'))
        .unwrap_or("");
    match ext.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "xml" => "application/xml",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}
