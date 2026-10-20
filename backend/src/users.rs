//! 账户管理（契约 §3 “账户管理”，仅 is_admin=1 可访问，非管理员 403）。

use axum::body::Bytes;
use axum::extract::rejection::{BytesRejection, PathRejection};
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::Json;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::auth;
use crate::auth::AuthExtension;
use crate::error::{parse_json, ApiError};
use crate::util::now_rfc3339;
use crate::AppState;

/// 用户列表 / 创建用户响应对象。
#[derive(Debug, Serialize, Clone)]
pub struct UserOut {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
    pub created_at: String,
}

/// 创建用户请求体。
#[derive(Debug, Deserialize)]
pub struct CreateUserInput {
    pub username: Option<String>,
    pub password: Option<String>,
}

const USER_COLUMNS: &str = "id, username, is_admin, created_at";

fn row_to_user(row: &rusqlite::Row) -> rusqlite::Result<UserOut> {
    Ok(UserOut {
        id: row.get(0)?,
        username: row.get(1)?,
        is_admin: row.get::<_, i64>(2)? != 0,
        created_at: row.get(3)?,
    })
}

/// `GET /api/users` — 用户数组。
pub(crate) async fn list(State(state): State<AppState>) -> Result<Json<Vec<UserOut>>, ApiError> {
    let conn = state.conn();
    // 用户列表按用户名字母顺序，便于查找（契约 §4）。
    let mut stmt = conn.prepare(&format!("SELECT {USER_COLUMNS} FROM users ORDER BY username ASC"))?;
    let rows = stmt.query_map([], row_to_user)?;
    let users = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Json(users))
}

/// `POST /api/users` — 创建普通账户（is_admin=false）；201；用户名重复 409。
pub(crate) async fn create(
    State(state): State<AppState>,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<UserOut>), ApiError> {
    let input: CreateUserInput = parse_json(body?)?;
    let username = input
        .username
        .ok_or_else(|| ApiError::bad_request("缺少字段: username"))?
        .trim()
        .to_owned();
    if username.is_empty() {
        return Err(ApiError::bad_request("用户名不能为空"));
    }
    let password = input
        .password
        .ok_or_else(|| ApiError::bad_request("缺少字段: password"))?;
    if password.is_empty() {
        return Err(ApiError::bad_request("密码不能为空"));
    }

    let hash = auth::hash_password(&password);
    let conn = state.conn();
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM users WHERE username = ?1",
            [&username],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Err(ApiError::conflict("用户名已存在"));
    }
    match conn.execute(
        "INSERT INTO users (username, password_hash, is_admin, created_at) VALUES (?1, ?2, 0, ?3)",
        rusqlite::params![username, hash, now_rfc3339()],
    ) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ffi::ErrorCode::ConstraintViolation =>
        {
            return Err(ApiError::conflict("用户名已存在"));
        }
        Err(err) => return Err(err.into()),
    }
    let id = conn.last_insert_rowid();
    let user = conn.query_row(
        &format!("SELECT {USER_COLUMNS} FROM users WHERE id = ?1"),
        [id],
        row_to_user,
    )?;
    Ok((StatusCode::CREATED, Json(user)))
}

/// `DELETE /api/users/{id}` — 204；删除自己 → 400（契约指定文案）；不存在 → 404。
///
/// 删除用户由外键级联删除其 sessions（契约 §1）。
pub(crate) async fn remove(
    State(state): State<AppState>,
    auth: AuthExtension,
    id: Result<Path<i64>, PathRejection>,
) -> Result<StatusCode, ApiError> {
    let Extension(ctx) = auth?;
    let id = id?.0;
    if id == ctx.user.id {
        return Err(ApiError::bad_request("不能删除当前登录账户"));
    }
    let conn = state.conn();
    let affected = conn.execute("DELETE FROM users WHERE id = ?1", [id])?;
    if affected == 0 {
        return Err(ApiError::not_found("用户不存在"));
    }
    Ok(StatusCode::NO_CONTENT)
}
