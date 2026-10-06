//! SQLite 打开/建表/admin 幂等初始化（契约 §1）。

use std::path::Path;
use std::time::Duration;

use rusqlite::OptionalExtension;
use rusqlite::Connection;

use crate::auth;
use crate::util;

pub(crate) const DEFAULT_ADMIN_USERNAME: &str = "admin";
pub(crate) const DEFAULT_ADMIN_PASSWORD: &str = "admin123";

/// 打开数据库文件（自动创建父目录），执行 PRAGMA、建表与 admin 幂等初始化。
pub(crate) fn open(path: &Path) -> Result<Connection, Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.busy_timeout(Duration::from_secs(5))?;
    migrate(&conn)?;
    init_admin_from_env(&conn)?;
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
            expiry_date   TEXT,
            opened_at     TEXT,
            finished_at   TEXT,
            status        TEXT NOT NULL DEFAULT 'in_stock',
            notes         TEXT,
            created_at    TEXT NOT NULL,
            updated_at    TEXT NOT NULL
        );
        "#,
    )
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
        .query_row("SELECT id FROM users WHERE username = ?1", [username], |row| {
            row.get(0)
        })
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
        eprintln!(
            "[init] 已创建默认管理员 '{username}'（使用默认初始密码，登录后请立即修改密码）"
        );
    } else {
        eprintln!(
            "[init] 已创建管理员 '{username}'（初始密码已通过 ADMIN_PASSWORD 设置，登录后请立即修改密码）"
        );
    }
    Ok(true)
}

/// 读取 `ADMIN_USERNAME` / `ADMIN_PASSWORD`（空值视为未设置）后执行初始化。
pub(crate) fn init_admin_from_env(conn: &Connection) -> Result<bool, rusqlite::Error> {
    let username = env_or("ADMIN_USERNAME", DEFAULT_ADMIN_USERNAME);
    let password = env_or("ADMIN_PASSWORD", DEFAULT_ADMIN_PASSWORD);
    init_admin(conn, &username, &password)
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
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
            .query_row("SELECT is_admin FROM users WHERE username = 'admin'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(is_admin, 1);

        // 重复初始化 → 不创建、不覆盖既有密码
        assert!(!init_admin(&conn, "admin", "other-password").unwrap());
        assert!(verify_password(DEFAULT_ADMIN_PASSWORD, &stored_hash(&conn, "admin")));
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
        assert!(verify_password("user-changed-999", &stored_hash(&conn, "admin")));
        assert!(!verify_password(DEFAULT_ADMIN_PASSWORD, &stored_hash(&conn, "admin")));
    }

    #[test]
    fn init_admin_supports_custom_username_and_password() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        assert!(init_admin(&conn, "root", "custom-secret").unwrap());
        assert!(verify_password("custom-secret", &stored_hash(&conn, "root")));
        let is_admin: i64 = conn
            .query_row("SELECT is_admin FROM users WHERE username = 'root'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(is_admin, 1);
        // 再次以默认账号初始化 → admin 缺失，按规则补建；root 不受影响
        assert!(init_admin(&conn, "admin", DEFAULT_ADMIN_PASSWORD).unwrap());
        assert!(verify_password("custom-secret", &stored_hash(&conn, "root")));
    }

    /// 便于 db 测试引用（crate::auth 的验证函数）。
    fn verify_password(password: &str, hash: &str) -> bool {
        auth::verify_password(password, hash)
    }
}
