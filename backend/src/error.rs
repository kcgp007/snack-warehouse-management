//! 统一错误类型：所有失败响应均为 `{"error": "人类可读信息"}`（契约 §2）。

use axum::body::Bytes;
use axum::extract::rejection::{BytesRejection, ExtensionRejection, PathRejection, QueryRejection};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

/// API 错误：状态码 + 统一 JSON 错误体。
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }

    pub fn internal() -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "服务器内部错误")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

impl From<BytesRejection> for ApiError {
    fn from(_: BytesRejection) -> Self {
        Self::bad_request("读取请求体失败")
    }
}

impl From<QueryRejection> for ApiError {
    fn from(_: QueryRejection) -> Self {
        Self::bad_request("查询参数无效")
    }
}

impl From<PathRejection> for ApiError {
    fn from(_: PathRejection) -> Self {
        Self::bad_request("路径参数无效")
    }
}

impl From<ExtensionRejection> for ApiError {
    fn from(_: ExtensionRejection) -> Self {
        Self::unauthorized("未登录或登录已过期")
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(err: rusqlite::Error) -> Self {
        eprintln!("[db-error] {err}");
        Self::internal()
    }
}

/// 解析 JSON 请求体。
///
/// 不检查 Content-Type（契约要求统一 JSON 错误体，且便于 curl 冒烟测试直接 `-d` 提交）。
/// 缺字段 / 类型不匹配 → 400。
pub(crate) fn parse_json<T: serde::de::DeserializeOwned>(body: Bytes) -> Result<T, ApiError> {
    serde_json::from_slice(&body)
        .map_err(|_| ApiError::bad_request("请求体格式错误：需要 JSON 对象且字段类型正确"))
}
