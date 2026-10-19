//! 时间/日期与 token 工具。
//!
//! 契约要点：
//! - `created_at`/`updated_at`/`expires_at` 为 RFC3339 UTC；
//! - 过期判断（reminders、expiry_status 筛选）基于**服务器本地日期**的 `YYYY-MM-DD` 字符串比较；
//! - 会话 token ≥32 字节熵。

use chrono::{Local, NaiveDate, TimeDelta, Utc};
use rand::RngCore;

/// 当前 UTC 时间，RFC3339 格式。
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

/// 服务器本地日期 `YYYY-MM-DD`。
pub fn today_string() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

/// 服务器本地日期 + n 天，`YYYY-MM-DD`。
pub fn today_plus_days(days: i64) -> String {
    (Local::now().date_naive() + TimeDelta::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

/// 目标日期距本地今天的天数（已过期为负数）；日期非法返回 `None`。
pub fn days_until(date: &str) -> Option<i64> {
    let target = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    Some((target - Local::now().date_naive()).num_days())
}

/// 严格校验 `YYYY-MM-DD`（格式 + 真实日历日期）。
pub fn is_valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4]
            .iter()
            .chain(&b[5..7])
            .chain(&b[8..10])
            .all(u8::is_ascii_digit)
        && NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

/// 是否为 `/api` 或 `/api/...`。用边界判断避免把 `/apixxx` 误当 API 路径。
pub(crate) fn is_api_path(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}

/// 是否为 `/api/users` 或 `/api/users/...`（管理员专属前缀）。
pub(crate) fn is_users_path(path: &str) -> bool {
    path == "/api/users" || path.starts_with("/api/users/")
}

/// 生成会话 token：32 字节（256 位）随机熵，十六进制编码（64 字符）。
pub fn new_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_validation() {
        assert!(is_valid_date("2026-01-05"));
        assert!(!is_valid_date("2026/01/05"));
        assert!(!is_valid_date("2026-1-05"));
        assert!(!is_valid_date("2026-13-05"));
        assert!(!is_valid_date("2026-02-30"));
        assert!(!is_valid_date("2026-01-05T08:30"));
        assert!(!is_valid_date("2026-01-05 08:30"));
        assert!(!is_valid_date(""));
    }

    #[test]
    fn api_path_boundaries() {
        assert!(is_api_path("/api"));
        assert!(is_api_path("/api/snacks"));
        assert!(!is_api_path("/apixxx"));
        assert!(!is_api_path("/apiary"));
        assert!(!is_api_path("/"));
        assert!(is_users_path("/api/users"));
        assert!(is_users_path("/api/users/1"));
        assert!(!is_users_path("/api/usersfoo"));
        assert!(!is_users_path("/api/snacks"));
    }

    #[test]
    fn session_token_has_enough_entropy() {
        let token = new_session_token();
        assert_eq!(token.len(), 64); // 32 字节 → 64 hex 字符
        assert_ne!(token, new_session_token());
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
