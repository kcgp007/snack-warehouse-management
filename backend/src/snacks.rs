//! 零食 CRUD、查询过滤与存放位置列表（契约 §3 “零食”）。

use axum::body::Bytes;
use axum::extract::rejection::{BytesRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use rusqlite::types::Value;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::error::{parse_json, ApiError};
use crate::util::{is_valid_date, now_rfc3339, today_plus_days, today_string};
use crate::AppState;

/// 生命周期状态枚举（契约 §1 snacks.status）。
pub(crate) const LIFECYCLE_STATUSES: [&str; 3] = ["in_stock", "opened", "finished"];

const SELECT_COLUMNS: &str = "id, name, quantity, location, purchase_date, expiry_date, opened_at, finished_at, status, notes, created_at, updated_at";

/// Snack = SnackInput 字段 + id/created_at/updated_at（契约 §3）。
#[derive(Debug, Clone, Serialize)]
pub struct Snack {
    pub id: i64,
    pub name: String,
    pub quantity: i64,
    pub location: String,
    pub purchase_date: Option<String>,
    pub expiry_date: Option<String>,
    pub opened_at: Option<String>,
    pub finished_at: Option<String>,
    pub status: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// SnackInput 原始请求体：三个核心字段必填，其余可省略/null。
#[derive(Debug, Deserialize)]
pub(crate) struct SnackInputRaw {
    pub name: Option<String>,
    pub quantity: Option<i64>,
    pub location: Option<String>,
    pub purchase_date: Option<String>,
    pub expiry_date: Option<String>,
    pub opened_at: Option<String>,
    pub finished_at: Option<String>,
    pub status: Option<String>,
    pub notes: Option<String>,
}

/// 校验通过后的输入（契约 §3 校验规则）。
#[derive(Debug)]
pub(crate) struct SnackInput {
    pub name: String,
    pub quantity: i64,
    pub location: String,
    pub purchase_date: Option<String>,
    pub expiry_date: Option<String>,
    pub opened_at: Option<String>,
    pub finished_at: Option<String>,
    pub status: String,
    pub notes: Option<String>,
}

/// 校验 SnackInput（缺字段 / quantity<0 / 日期格式非法 / status 非法 → 400）。
///
/// 服务端规则：`status="finished"` 且未带 `finished_at` 时自动填充当前本地日期（契约 §3）。
fn validate(raw: SnackInputRaw) -> Result<SnackInput, ApiError> {
    let name = raw
        .name
        .ok_or_else(|| ApiError::bad_request("缺少字段: name"))?
        .trim()
        .to_owned();
    if name.is_empty() {
        return Err(ApiError::bad_request("名称不能为空"));
    }

    let quantity = raw
        .quantity
        .ok_or_else(|| ApiError::bad_request("缺少字段: quantity"))?;
    if quantity < 0 {
        return Err(ApiError::bad_request("数量不能为负数"));
    }

    let location = raw
        .location
        .ok_or_else(|| ApiError::bad_request("缺少字段: location"))?
        .trim()
        .to_owned();
    if location.is_empty() {
        return Err(ApiError::bad_request("存放位置不能为空"));
    }

    let purchase_date = optional_date(raw.purchase_date, "purchase_date")?;
    let expiry_date = optional_date(raw.expiry_date, "expiry_date")?;
    let opened_at = optional_date(raw.opened_at, "opened_at")?;
    let mut finished_at = optional_date(raw.finished_at, "finished_at")?;

    let status = raw.status.unwrap_or_else(|| "in_stock".to_owned());
    if !LIFECYCLE_STATUSES.contains(&status.as_str()) {
        return Err(ApiError::bad_request(
            "status 必须是 in_stock|opened|finished",
        ));
    }
    if status == "finished" && finished_at.is_none() {
        finished_at = Some(today_string());
    }

    Ok(SnackInput {
        name,
        quantity,
        location,
        purchase_date,
        expiry_date,
        opened_at,
        finished_at,
        status,
        notes: raw.notes,
    })
}

fn optional_date(value: Option<String>, field: &str) -> Result<Option<String>, ApiError> {
    match value {
        None => Ok(None),
        Some(s) if s.is_empty() => Ok(None),
        Some(s) if is_valid_date(&s) => Ok(Some(s)),
        Some(_) => Err(ApiError::bad_request(format!(
            "{field} 日期格式必须为 YYYY-MM-DD"
        ))),
    }
}

pub(crate) fn row_to_snack(row: &Row) -> rusqlite::Result<Snack> {
    Ok(Snack {
        id: row.get(0)?,
        name: row.get(1)?,
        quantity: row.get(2)?,
        location: row.get(3)?,
        purchase_date: row.get(4)?,
        expiry_date: row.get(5)?,
        opened_at: row.get(6)?,
        finished_at: row.get(7)?,
        status: row.get(8)?,
        notes: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

/// `GET /api/snacks` 查询参数（契约 §3）：
/// `q`（name/notes 模糊）、`location`（精确）、
/// `expiry_status`（过期维度 all|ok|expiring|expired，默认 all）、
/// `status`（生命周期维度 all|in_stock|opened|finished，默认 all）。
#[derive(Debug, Deserialize, Default)]
pub(crate) struct SnackListQuery {
    pub q: Option<String>,
    pub location: Option<String>,
    pub expiry_status: Option<String>,
    pub status: Option<String>,
}

/// `GET /api/snacks` — 返回裸数组。
pub(crate) async fn list(
    State(state): State<AppState>,
    query: Result<Query<SnackListQuery>, QueryRejection>,
) -> Result<Json<Vec<Snack>>, ApiError> {
    let query = query?.0;
    let expiry_status = query.expiry_status.as_deref().unwrap_or("all");
    let status = query.status.as_deref().unwrap_or("all");
    if !["all", "ok", "expiring", "expired"].contains(&expiry_status) {
        return Err(ApiError::bad_request(
            "expiry_status 必须是 all|ok|expiring|expired",
        ));
    }
    if status != "all" && !LIFECYCLE_STATUSES.contains(&status) {
        return Err(ApiError::bad_request(
            "status 必须是 all|in_stock|opened|finished",
        ));
    }

    let mut wheres: Vec<String> = Vec::new();
    let mut binds: Vec<Value> = Vec::new();

    if let Some(term) = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        // ESCAPE 必须显式声明，否则 `escape_like` 写入的 `\%` / `\_` 只是普通字符，
        // `%`/`_` 仍会被 SQLite 当通配符（搜 `100%`/`a_b` 会漏检或误命中）。
        wheres.push("(name LIKE ? ESCAPE '\\' OR notes LIKE ? ESCAPE '\\')".to_owned());
        let pattern = format!("%{}%", escape_like(term));
        binds.push(Value::Text(pattern.clone()));
        binds.push(Value::Text(pattern));
    }
    if let Some(location) = query
        .location
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        wheres.push("location = ?".to_owned());
        binds.push(Value::Text(location.to_owned()));
    }

    // 过期维度：基于本地日期字符串比较（契约 §3）。
    match expiry_status {
        "ok" => {
            wheres.push("(expiry_date IS NULL OR expiry_date > ?)".to_owned());
            binds.push(Value::Text(today_plus_days(7)));
        }
        "expiring" => {
            wheres.push(
                "(expiry_date IS NOT NULL AND expiry_date >= ? AND expiry_date <= ?)".to_owned(),
            );
            binds.push(Value::Text(today_string()));
            binds.push(Value::Text(today_plus_days(7)));
        }
        "expired" => {
            wheres.push("(expiry_date IS NOT NULL AND expiry_date < ?)".to_owned());
            binds.push(Value::Text(today_string()));
        }
        _ => {}
    }

    // 生命周期维度。
    if status != "all" {
        wheres.push("status = ?".to_owned());
        binds.push(Value::Text(status.to_owned()));
    }

    let mut sql = format!("SELECT {SELECT_COLUMNS} FROM snacks");
    if !wheres.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&wheres.join(" AND "));
    }
    sql.push_str(" ORDER BY id ASC");

    let conn = state.conn();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(binds.iter()), row_to_snack)?;
    let snacks = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Json(snacks))
}

/// 转义 LIKE 通配符（`\` 为 ESCAPE 字符）。
fn escape_like(term: &str) -> String {
    term.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// `POST /api/snacks` — 201 返回 Snack。
pub(crate) async fn create(
    State(state): State<AppState>,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<Snack>), ApiError> {
    let input = validate(parse_json(body?)?)?;
    let now = now_rfc3339();
    let conn = state.conn();
    conn.execute(
        "INSERT INTO snacks (name, quantity, location, purchase_date, expiry_date, \
         opened_at, finished_at, status, notes, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        params![
            input.name,
            input.quantity,
            input.location,
            input.purchase_date,
            input.expiry_date,
            input.opened_at,
            input.finished_at,
            input.status,
            input.notes,
            now
        ],
    )?;
    let id = conn.last_insert_rowid();
    let snack = conn.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM snacks WHERE id = ?1"),
        [id],
        row_to_snack,
    )?;
    Ok((StatusCode::CREATED, Json(snack)))
}

/// `PUT /api/snacks/{id}` — 全量替换；不存在 404。
pub(crate) async fn update(
    State(state): State<AppState>,
    id: Result<Path<i64>, PathRejection>,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<Snack>, ApiError> {
    let id = id?.0;
    let input = validate(parse_json(body?)?)?;
    let now = now_rfc3339();
    let conn = state.conn();
    let affected = conn.execute(
        "UPDATE snacks SET name = ?1, quantity = ?2, location = ?3, purchase_date = ?4, \
         expiry_date = ?5, opened_at = ?6, finished_at = ?7, status = ?8, notes = ?9, \
         updated_at = ?10 WHERE id = ?11",
        params![
            input.name,
            input.quantity,
            input.location,
            input.purchase_date,
            input.expiry_date,
            input.opened_at,
            input.finished_at,
            input.status,
            input.notes,
            now,
            id
        ],
    )?;
    if affected == 0 {
        return Err(ApiError::not_found("零食不存在"));
    }
    let snack = conn.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM snacks WHERE id = ?1"),
        [id],
        row_to_snack,
    )?;
    Ok(Json(snack))
}

/// `DELETE /api/snacks/{id}` — 204；不存在 404。
pub(crate) async fn remove(
    State(state): State<AppState>,
    id: Result<Path<i64>, PathRejection>,
) -> Result<StatusCode, ApiError> {
    let id = id?.0;
    let conn = state.conn();
    let affected = conn.execute("DELETE FROM snacks WHERE id = ?1", [id])?;
    if affected == 0 {
        return Err(ApiError::not_found("零食不存在"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/locations` 查询参数：`q` 为位置子串（缺省/空串返回全部）。
#[derive(Debug, Deserialize, Default)]
pub(crate) struct LocationQuery {
    pub q: Option<String>,
}

/// `GET /api/locations?q=` — 去重后的存放位置，按字典序；
/// `q` 按子串包含过滤（契约 §3），缺省或空串返回全部。
pub(crate) async fn locations(
    State(state): State<AppState>,
    query: Result<Query<LocationQuery>, QueryRejection>,
) -> Result<Json<Vec<String>>, ApiError> {
    let term = query?
        .0
        .q
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    let conn = state.conn();
    let values = match term {
        None => {
            let mut stmt =
                conn.prepare("SELECT DISTINCT location FROM snacks ORDER BY location")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
        Some(term) => {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT location FROM snacks WHERE location LIKE ? ESCAPE '\\' \
                 ORDER BY location",
            )?;
            let pattern = format!("%{}%", escape_like(&term));
            let rows = stmt.query_map([pattern], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
    };
    Ok(Json(values))
}
