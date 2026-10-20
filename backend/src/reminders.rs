//! 过期提醒（契约 §3 “过期提醒”）。

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::error::ApiError;
use crate::snacks::{row_to_snack, Snack, SELECT_COLUMNS};
use crate::util::{days_until, today_plus_days};
use crate::AppState;

/// 提醒条目：Snack 全字段 + `days_left` + `level`。
///
/// 分级字段名为 `level`（Snack 自身的 `status` 是生命周期状态，二者不可混用）。
#[derive(Debug, Serialize)]
pub struct Reminder {
    #[serde(flatten)]
    pub snack: Snack,
    pub days_left: i64,
    /// `expired`（已过期）| `critical`（≤3 天）| `warning`（≤7 天）。
    pub level: &'static str,
}

/// `GET /api/reminders` — 仅返回 `expiry_date` 非空且 `days_left ≤ 7`、
/// 生命周期 `status != "finished"` 的零食，按 `expiry_date` 升序（同日期按 id）。
pub(crate) async fn list(State(state): State<AppState>) -> Result<Json<Vec<Reminder>>, ApiError> {
    let cutoff = today_plus_days(7);
    let conn = state.conn();
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM snacks \
         WHERE expiry_date IS NOT NULL AND expiry_date <= ?1 AND status != 'finished' \
         ORDER BY expiry_date ASC, id ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([&cutoff], row_to_snack)?;

    let mut reminders = Vec::new();
    for row in rows {
        let snack = row?;
        // 基于服务器本地日期计算剩余天数（契约 §3）。
        let Some(days_left) = snack.expiry_date.as_deref().and_then(days_until) else {
            continue;
        };
        let level = if days_left < 0 {
            "expired"
        } else if days_left <= 3 {
            "critical"
        } else {
            "warning"
        };
        reminders.push(Reminder {
            snack,
            days_left,
            level,
        });
    }
    Ok(Json(reminders))
}
