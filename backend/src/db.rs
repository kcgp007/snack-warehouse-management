//! SQLite 打开/建表/admin 幂等初始化（契约 §1）。

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;
use rusqlite::OptionalExtension;

use crate::auth;
use crate::util;

pub(crate) const DEFAULT_ADMIN_PASSWORD: &str = "admin123";

/// 打开数据库文件（自动创建父目录），执行 PRAGMA、建表与 admin 幂等初始化。
pub(crate) fn open(
    path: &Path,
    admin_username: &str,
    admin_password: &str,
) -> Result<Connection, Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.busy_timeout(Duration::from_secs(5))?;
    migrate(&conn)?;
    init_admin(&conn, admin_username, admin_password)?;
    Ok(conn)
}

/// 建表（幂等）。snacks 含契约 §1 的生命周期字段 status/opened_at/finished_at。
fn migrate(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            username      TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            is_admin      INTEGER NOT NULL DEFAULT 0,
            created_at    TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sessions (
            token      TEXT PRIMARY KEY,
            user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            expires_at TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS snacks (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            name          TEXT NOT NULL,
            quantity      INTEGER NOT NULL DEFAULT 1,
            location      TEXT NOT NULL,
            purchase_date TEXT,
            production_date TEXT,
            shelf_life_days INTEGER,
            shelf_life_unit TEXT NOT NULL DEFAULT 'day',
            expiry_date   TEXT,
            opened_at     TEXT,
            finished_at   TEXT,
            status        TEXT NOT NULL DEFAULT 'in_stock',
            notes         TEXT,
            created_at    TEXT NOT NULL,
            updated_at    TEXT NOT NULL
        );
        "#,
    )?;
    ensure_snack_columns(conn)?;
    normalize_legacy_lifecycle_dates(conn)?;
    purge_expired_sessions(conn)?;
    Ok(())
}

/// 为 v1/v2 存量数据库补齐 v3 新增列（幂等）。
///
/// `CREATE TABLE IF NOT EXISTS` 不会修改已存在的表，因此升级安装必须显式
/// `ALTER TABLE ... ADD COLUMN`；用 `PRAGMA table_info` 判断可重复执行。
fn ensure_snack_columns(conn: &Connection) -> Result<(), rusqlite::Error> {
    let columns: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(snacks)")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let has = |name: &str| columns.iter().any(|column| column == name);
    if !has("production_date") {
        conn.execute_batch("ALTER TABLE snacks ADD COLUMN production_date TEXT;")?;
    }
    if !has("shelf_life_days") {
        conn.execute_batch("ALTER TABLE snacks ADD COLUMN shelf_life_days INTEGER;")?;
    }
    if !has("shelf_life_unit") {
        conn.execute_batch(
            "ALTER TABLE snacks ADD COLUMN shelf_life_unit TEXT NOT NULL DEFAULT 'day';",
        )?;
    }
    Ok(())
}

/// 启动时清理已过期会话（`sessions` 表不会随登录/退出无限增长）。
///
/// 会话中间件在请求时也会顺手删除过期 token；这里额外在启动时回收，
/// 让长期不下线的实例也有机会清掉陈旧记录。
fn purge_expired_sessions(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM sessions WHERE expires_at < ?1",
        [util::now_rfc3339()],
    )?;
    Ok(())
}

/// v1 存量数据迁移（契约 v2）：`opened_at`/`finished_at` 统一为纯日期 `YYYY-MM-DD`。
///
/// - 已是合法纯日期：保持不变；
/// - 旧格式（如 `YYYY-MM-DDTHH:MM`）：截取前 10 位，若为合法日期则写入；
/// - 无法解析/非法日期：置 `NULL`。
///
/// 迁移幂等：第二次启动时所有值均已合法，不会产生 UPDATE。
fn normalize_legacy_lifecycle_dates(conn: &Connection) -> Result<(), rusqlite::Error> {
    let rows: Vec<(i64, Option<String>, Option<String>, bool)> = {
        let mut stmt = conn.prepare("SELECT id, opened_at, finished_at FROM snacks")?;
        // 用 `ValueRef` 读取而不是 `String`：即便存量列里混入 BLOB 等非文本类型，
        // 也按“非法值置 NULL”处理，而不是让启动过程直接失败。
        // `types_ok` 标记两个值的 SQLite 类型是否本就是 TEXT/NULL。
        let mapped = stmt.query_map([], |row| {
            let (opened_text, opened_at) = text_value(row.get_ref(1)?);
            let (finished_text, finished_at) = text_value(row.get_ref(2)?);
            Ok((
                row.get(0)?,
                opened_at,
                finished_at,
                opened_text && finished_text,
            ))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };

    for (id, opened_at, finished_at, types_ok) in rows {
        let normalized_opened = opened_at.as_deref().and_then(normalize_legacy_date);
        let normalized_finished = finished_at.as_deref().and_then(normalize_legacy_date);
        if !types_ok || normalized_opened != opened_at || normalized_finished != finished_at {
            conn.execute(
                "UPDATE snacks SET opened_at = ?1, finished_at = ?2 WHERE id = ?3",
                rusqlite::params![normalized_opened, normalized_finished, id],
            )?;
        }
    }
    Ok(())
}

/// 将单个存量值规范化为 `YYYY-MM-DD`；非法值返回 `None`（写入 NULL）。
fn normalize_legacy_date(value: &str) -> Option<String> {
    if util::is_valid_date(value) {
        return Some(value.to_owned());
    }
    let prefix: String = value.chars().take(10).collect();
    if util::is_valid_date(&prefix) {
        Some(prefix)
    } else {
        None
    }
}

/// 读取单个列的值：返回 `(值本是 TEXT/NULL 类型, 文本值)`。
/// BLOB/INTEGER/REAL 等一律视为非法（`(false, None)`），由调用方写入 NULL。
fn text_value(value: rusqlite::types::ValueRef<'_>) -> (bool, Option<String>) {
    match value {
        rusqlite::types::ValueRef::Text(bytes) => {
            (true, Some(String::from_utf8_lossy(bytes).into_owned()))
        }
        rusqlite::types::ValueRef::Null => (true, None),
        _ => (false, None),
    }
}

/// 幂等初始化管理员（契约 §1）：
///
/// - `username` 用户不存在则创建（`is_admin=1`）；
/// - 已存在同名用户时**绝不覆盖**其 `password_hash`（即使用户改过密码）；
/// - 初始化发生时打印一行日志（含用户名，不打印任何密码明文；
///   非默认密码仅提示来自 `ADMIN_PASSWORD`）。
///
/// 返回是否发生了创建。
pub(crate) fn init_admin(
    conn: &Connection,
    username: &str,
    password: &str,
) -> Result<bool, rusqlite::Error> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM users WHERE username = ?1",
            [username],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Ok(false);
    }

    let hash = auth::hash_password(password);
    conn.execute(
        "INSERT INTO users (username, password_hash, is_admin, created_at) VALUES (?1, ?2, 1, ?3)",
        rusqlite::params![username, hash, util::now_rfc3339()],
    )?;
    if password == DEFAULT_ADMIN_PASSWORD {
        eprintln!("[init] 已创建默认管理员 '{username}'（使用默认初始密码，登录后请立即修改密码）");
    } else {
        eprintln!(
            "[init] 已创建管理员 '{username}'（初始密码已通过 ADMIN_PASSWORD 设置，登录后请立即修改密码）"
        );
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored_hash(conn: &Connection, username: &str) -> String {
        conn.query_row(
            "SELECT password_hash FROM users WHERE username = ?1",
            [username],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn init_admin_creates_on_empty_db_and_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        // 空库 → 创建默认管理员
        assert!(init_admin(&conn, "admin", DEFAULT_ADMIN_PASSWORD).unwrap());
        let is_admin: i64 = conn
            .query_row(
                "SELECT is_admin FROM users WHERE username = 'admin'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(is_admin, 1);

        // 重复初始化 → 不创建、不覆盖既有密码
        assert!(!init_admin(&conn, "admin", "other-password").unwrap());
        assert!(verify_password(
            DEFAULT_ADMIN_PASSWORD,
            &stored_hash(&conn, "admin")
        ));
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn init_admin_never_resets_a_changed_password() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        assert!(init_admin(&conn, "admin", DEFAULT_ADMIN_PASSWORD).unwrap());

        // 模拟用户改密
        let changed = auth::hash_password("user-changed-999");
        conn.execute(
            "UPDATE users SET password_hash = ?1 WHERE username = 'admin'",
            [&changed],
        )
        .unwrap();

        // 模拟“重启”再次执行初始化 → 密码保持用户改过的值
        assert!(!init_admin(&conn, "admin", DEFAULT_ADMIN_PASSWORD).unwrap());
        assert!(verify_password(
            "user-changed-999",
            &stored_hash(&conn, "admin")
        ));
        assert!(!verify_password(
            DEFAULT_ADMIN_PASSWORD,
            &stored_hash(&conn, "admin")
        ));
    }

    #[test]
    fn init_admin_supports_custom_username_and_password() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        assert!(init_admin(&conn, "root", "custom-secret").unwrap());
        assert!(verify_password(
            "custom-secret",
            &stored_hash(&conn, "root")
        ));
        let is_admin: i64 = conn
            .query_row(
                "SELECT is_admin FROM users WHERE username = 'root'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(is_admin, 1);
        // 再次以默认账号初始化 → admin 缺失，按规则补建；root 不受影响
        assert!(init_admin(&conn, "admin", DEFAULT_ADMIN_PASSWORD).unwrap());
        assert!(verify_password(
            "custom-secret",
            &stored_hash(&conn, "root")
        ));
    }

    /// 便于 db 测试引用（crate::auth 的验证函数）。
    fn verify_password(password: &str, hash: &str) -> bool {
        auth::verify_password(password, hash)
    }

    #[test]
    fn legacy_datetime_values_are_normalized_and_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO snacks (name, quantity, location, opened_at, finished_at, status, \
             created_at, updated_at) VALUES \
             ('旧数据1', 1, '厨房', '2026-03-01T09:30', '2026-03-02 18:00', 'finished', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO snacks (name, quantity, location, opened_at, finished_at, status, \
             created_at, updated_at) VALUES \
             ('旧数据2', 1, '厨房', 'not-a-date', '2026-02-30T10:00', 'opened', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        normalize_legacy_lifecycle_dates(&conn).unwrap();

        let first: (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT opened_at, finished_at FROM snacks WHERE name = '旧数据1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            first,
            (Some("2026-03-01".to_owned()), Some("2026-03-02".to_owned()))
        );

        let second: (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT opened_at, finished_at FROM snacks WHERE name = '旧数据2'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(second, (None, None), "非法值应置 NULL");

        // 幂等：再次迁移不改变结果。
        normalize_legacy_lifecycle_dates(&conn).unwrap();
        let again: (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT opened_at, finished_at FROM snacks WHERE name = '旧数据1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(again, first);
    }

    #[test]
    fn expired_sessions_are_purged_on_startup() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        init_admin(&conn, "admin", DEFAULT_ADMIN_PASSWORD).unwrap();
        let user_id: i64 = conn
            .query_row("SELECT id FROM users WHERE username = 'admin'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let past = (chrono::Utc::now() - chrono::TimeDelta::days(1)).to_rfc3339();
        let future = (chrono::Utc::now() + chrono::TimeDelta::days(1)).to_rfc3339();
        let created = util::now_rfc3339();
        conn.execute(
            "INSERT INTO sessions (token, user_id, expires_at, created_at) \
             VALUES ('stale', ?1, ?2, ?4), ('fresh', ?1, ?3, ?4)",
            rusqlite::params![user_id, past, future, created],
        )
        .unwrap();

        // 模拟重启：再次 migrate 应删掉过期 session、保留未过期的。
        migrate(&conn).unwrap();

        let tokens: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT token FROM sessions ORDER BY token")
                .unwrap();
            let mapped = stmt.query_map([], |row| row.get::<_, String>(0)).unwrap();
            mapped.collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        assert_eq!(tokens, vec!["fresh".to_owned()]);
    }

    #[test]
    fn non_text_legacy_values_do_not_break_migration() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO snacks (name, quantity, location, opened_at, finished_at, status, \
             created_at, updated_at) VALUES \
             ('blob', 1, '厨房', X'DEADBEEF', NULL, 'opened', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        // 不应因 BLOB 值而启动失败；非法值置 NULL。
        migrate(&conn).unwrap();
        let opened: Option<String> = conn
            .query_row(
                "SELECT opened_at FROM snacks WHERE name = 'blob'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(opened, None);
    }
}
