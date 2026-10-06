//! 认证与授权（契约 §2、§3）：登录/登出/当前用户/改密，以及全局认证中间件。

use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::Argon2;
use axum::body::Bytes;
use axum::extract::rejection::{BytesRejection, ExtensionRejection};
use axum::extract::{Extension, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::error::{parse_json, ApiError};
use crate::util::{new_session_token, now_rfc3339};
use crate::AppState;

/// 会话有效期：签发后 7 天（契约 §1 sessions）。
const SESSION_DAYS: i64 = 7;

/// argon2id 密码哈希（契约 §1：argon2 或 bcrypt，禁止明文/自研）。
pub(crate) fn hash_password(password: &str) -> String {
    Argon2::default()
        .hash_password(password.as_bytes())
        .expect("argon2 password hashing failed")
        .to_string()
}

/// 校验密码；哈希串损坏或不匹配均返回 false。
pub(crate) fn verify_password(password: &str, stored_hash: &str) -> bool {
    Argon2::default()
        .verify_password(password.as_bytes(), stored_hash)
        .is_ok()
}

/// 已认证用户（由中间件写入 request extensions）。
#[derive(Clone, Debug)]
pub(crate) struct CurrentUser {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
}

/// 认证上下文：当前用户 + 本次请求使用的 token（登出时需要）。
#[derive(Clone, Debug)]
pub(crate) struct AuthContext {
    pub user: CurrentUser,
    pub token: String,
}

/// 提取当前认证上下文的参数类型：缺失/异常统一转 401。
pub(crate) type AuthExtension = Result<Extension<AuthContext>, ExtensionRejection>;

/// 认证与授权中间件（契约 §2）：
/// - 非 `/api` 路径直接放行（静态资源无需登录）；
/// - `POST /api/auth/login` 放行；
/// - 其余 `/api/*` 必须携带有效 Bearer token，否则 401 `{"error":...}`；
/// - `/api/users*` 管理员专属，非管理员 403。
pub(crate) async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_owned();
    if !path.starts_with("/api") {
        return next.run(req).await;
    }
    if path == "/api/auth/login" && req.method().as_str() == "POST" {
        return next.run(req).await;
    }

    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (scheme, rest) = value.split_once(' ')?;
            scheme
                .eq_ignore_ascii_case("bearer")
                .then(|| rest.trim().to_owned())
        });
    let Some(token) = token else {
        return ApiError::unauthorized("缺少认证令牌").into_response();
    };

    let row = {
        let conn = state.conn();
        conn.query_row(
            "SELECT u.id, u.username, u.is_admin, s.expires_at \
             FROM sessions s JOIN users u ON u.id = s.user_id \
             WHERE s.token = ?1",
            [&token],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
    };
    let (user_id, username, is_admin, expires_at) = match row {
        Ok(row) => row,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return ApiError::unauthorized("登录已过期或令牌无效").into_response();
        }
        Err(err) => {
            eprintln!("[auth-error] {err}");
            return ApiError::internal().into_response();
        }
    };

    let expired = chrono::DateTime::parse_from_rfc3339(&expires_at)
        .map(|t| t < chrono::Utc::now())
        .unwrap_or(true);
    if expired {
        {
            let conn = state.conn();
            let _ = conn.execute("DELETE FROM sessions WHERE token = ?1", [&token]);
        }
        return ApiError::unauthorized("登录已过期或令牌无效").into_response();
    }

    if path.starts_with("/api/users") && is_admin == 0 {
        return ApiError::forbidden("需要管理员权限").into_response();
    }

    req.extensions_mut().insert(AuthContext {
        user: CurrentUser {
            id: user_id,
            username,
            is_admin: is_admin != 0,
        },
        token,
    });
    next.run(req).await
}

/// 未匹配的 `/api/*` 请求（带有效 token）→ 统一 404 JSON。
pub(crate) async fn api_not_found() -> ApiError {
    ApiError::not_found("接口不存在")
}

/// 登录请求体。字段缺失不返回 400，而是按“失败 401”统一处理。
#[derive(Debug, Deserialize)]
pub struct LoginInput {
    pub username: Option<String>,
    pub password: Option<String>,
}

/// 登录成功响应。
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserPublic,
}

/// 当前用户对象（登录 / me 共用）。
#[derive(Debug, Serialize, Clone)]
pub struct UserPublic {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
}

/// `POST /api/auth/login` — 成功 200 `{token, user}`；任何失败一律 401（契约 §3）。
pub(crate) async fn login(
    State(state): State<AppState>,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<LoginResponse>, ApiError> {
    let unauthorized = || ApiError::unauthorized("用户名或密码错误");
    let body = match body {
        Ok(body) => body,
        Err(_) => return Err(unauthorized()),
    };
    let input: LoginInput = serde_json::from_slice(&body).map_err(|_| unauthorized())?;
    let username = input.username.unwrap_or_default();
    let password = input.password.unwrap_or_default();

    let row = {
        let conn = state.conn();
        conn.query_row(
            "SELECT id, username, password_hash, is_admin FROM users WHERE username = ?1",
            [&username],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()
    };
    let Some((id, username, stored_hash, is_admin)) = row.map_err(|err| {
        eprintln!("[db-error] {err}");
        ApiError::internal()
    })?
    else {
        return Err(unauthorized());
    };
    if !verify_password(&password, &stored_hash) {
        return Err(unauthorized());
    }

    let token = new_session_token();
    let expires_at = (chrono::Utc::now() + chrono::TimeDelta::days(SESSION_DAYS)).to_rfc3339();
    {
        let conn = state.conn();
        conn.execute(
            "INSERT INTO sessions (token, user_id, expires_at, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![token, id, expires_at, now_rfc3339()],
        )?;
    }

    Ok(Json(LoginResponse {
        token,
        user: UserPublic {
            id,
            username,
            is_admin: is_admin != 0,
        },
    }))
}

/// `POST /api/auth/logout` — 使当前 token 失效，204。
pub(crate) async fn logout(
    State(state): State<AppState>,
    auth: AuthExtension,
) -> Result<StatusCode, ApiError> {
    let Extension(ctx) = auth?;
    {
        let conn = state.conn();
        conn.execute("DELETE FROM sessions WHERE token = ?1", [&ctx.token])?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/auth/me` — 返回当前登录用户对象。
pub(crate) async fn me(auth: AuthExtension) -> Result<Json<UserPublic>, ApiError> {
    let Extension(ctx) = auth?;
    Ok(Json(UserPublic {
        id: ctx.user.id,
        username: ctx.user.username.clone(),
        is_admin: ctx.user.is_admin,
    }))
}

/// 改密请求体。
#[derive(Debug, Deserialize)]
pub struct ChangePasswordInput {
    pub old_password: String,
    pub new_password: String,
}

/// `POST /api/auth/change-password` — 所有登录用户可改自己密码：
/// `new_password` <6 字符 → 400；原密码错误 → 400；成功 204。
pub(crate) async fn change_password(
    State(state): State<AppState>,
    auth: AuthExtension,
    body: Result<Bytes, BytesRejection>,
) -> Result<StatusCode, ApiError> {
    let Extension(ctx) = auth?;
    let input: ChangePasswordInput = parse_json(body?)?;
    if input.new_password.chars().count() < 6 {
        return Err(ApiError::bad_request("新密码长度至少 6 位"));
    }

    let conn = state.conn();
    let stored_hash: String = conn.query_row(
        "SELECT password_hash FROM users WHERE id = ?1",
        [ctx.user.id],
        |row| row.get(0),
    )?;
    if !verify_password(&input.old_password, &stored_hash) {
        return Err(ApiError::bad_request("原密码错误"));
    }
    let new_hash = hash_password(&input.new_password);
    conn.execute(
        "UPDATE users SET password_hash = ?1 WHERE id = ?2",
        rusqlite::params![new_hash, ctx.user.id],
    )?;
    Ok(StatusCode::NO_CONTENT)
}
