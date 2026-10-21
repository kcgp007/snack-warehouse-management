//! 后端集成测试：使用 axum `tower::ServiceExt::oneshot` 内嵌请求，不启动真实端口。
//!
//! 覆盖（契约 §5 + 两个契约增量）：
//! - 登录成功/失败、无 token 401、非管理员 /api/users 403
//! - snacks CRUD 与校验 400、GET /api/snacks 过滤（expiry_status/status 改名与新增）
//! - 搜索的 LIKE 通配符转义（`100%` / `a_b` 按字面量命中，`ESCAPE '\'` 回归）
//! - 生命周期 status 校验/筛选、finished 自动填充 finished_at
//! - reminders 分级（level 字段）、days_left 计算、排序、排除 finished
//! - admin 创建/删除用户、删除自己 400、change-password
//! - admin 自动初始化：空库创建 / 重启不重置改过的密码 / ADMIN_* 覆盖
//! - 静态托管与 SPA fallback（含目录不存在的优雅降级）
//!
//! 注意：所有测试共用进程级互斥锁串行执行，因为 admin 初始化会读取
//! 配置文件（snack-warehouse.toml）与 admin 幂等初始化测试。

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde::Deserialize;
use serde_json::{json, Value};
use snack_warehouse_backend::{build_app, AppConfig};
use tower::ServiceExt;

/// TOML 服务器配置（测试用：读取 snack-warehouse.toml 中的 admin 凭据覆盖）。
#[derive(Clone, Debug, Deserialize, Default)]
struct TomlServerConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub admin_username: Option<String>,
    pub admin_password: Option<String>,
}

// ---------------------------------------------------------------------------
// 测试基础设施
// ---------------------------------------------------------------------------

static TEST_LOCK: Mutex<()> = Mutex::new(());

fn test_guard() -> MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|err| err.into_inner())
}

/// 设置环境变量并在 Drop（含测试失败/panic）时恢复原值。
struct EnvGuard {
    saved: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn set(pairs: &[(&'static str, &str)]) -> Self {
        let mut saved = Vec::new();
        for (key, value) in pairs {
            saved.push((*key, std::env::var(key).ok()));
            std::env::set_var(key, value);
        }
        Self { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, old) in &self.saved {
            match old {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

/// 独立临时目录（数据库 + 静态目录），Drop 时清理。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("snackwh-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("创建临时目录");
        Self { path }
    }

    fn cfg(&self) -> AppConfig {
        let toml_path = self.path().join("snack-warehouse.toml");
        let overrides = if toml_path.exists() {
            let content = std::fs::read_to_string(&toml_path).unwrap_or_default();
            toml::from_str::<TomlServerConfig>(&content).unwrap_or_default()
        } else {
            TomlServerConfig::default()
        };
        AppConfig {
            db_path: self.path.join("app.db"),
            static_dir: self.path.join("static"),
            admin_username: overrides
                .admin_username
                .unwrap_or_else(|| "admin".to_owned()),
            admin_password: overrides
                .admin_password
                .unwrap_or_else(|| "admin123".to_owned()),
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

struct Resp {
    status: StatusCode,
    content_type: Option<String>,
    body: String,
}

impl Resp {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|err| panic!("响应不是 JSON（{err}）: {}", self.body))
    }
}

/// 发送原始字符串请求体。刻意**不设置** Content-Type，验证后端不依赖该头
/// （父级集成用 curl 冒烟时可直接 -d 提交）。
async fn send_raw(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<String>,
) -> Resp {
    let mut builder = Request::builder()
        .method(Method::from_bytes(method.as_bytes()).expect("方法"))
        .uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let req = builder
        .body(Body::from(body.unwrap_or_default()))
        .expect("构造请求");
    let resp = app.clone().oneshot(req).await.expect("oneshot");
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("读取响应体");
    Resp {
        status,
        content_type,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> Resp {
    send_raw(app, method, uri, token, body.map(|value| value.to_string())).await
}

/// 登录并断言成功，返回响应 JSON（含 token/user）。
async fn login(app: &Router, username: &str, password: &str) -> Value {
    let resp = send(
        app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({ "username": username, "password": password })),
    )
    .await;
    assert_eq!(
        resp.status,
        StatusCode::OK,
        "登录 {username} 失败: {}",
        resp.body
    );
    resp.json()
}

async fn token_of(app: &Router, username: &str, password: &str) -> String {
    let body = login(app, username, password).await;
    body.get("token")
        .and_then(Value::as_str)
        .expect("token 字段")
        .to_owned()
}

/// 服务器本地日期（相对偏移），与后端 reminders 计算使用同一时钟。
fn local_date(offset_days: i64) -> String {
    (chrono::Local::now().date_naive() + chrono::TimeDelta::days(offset_days))
        .format("%Y-%m-%d")
        .to_string()
}

fn snack(name: &str, location: &str, expiry_date: Option<&str>) -> Value {
    json!({
        "name": name,
        "quantity": 1,
        "location": location,
        "purchase_date": null,
        "production_date": null,
        "shelf_life_days": null,
        "expiry_date": expiry_date,
        "notes": null
    })
}

async fn create_snack(app: &Router, token: &str, payload: Value) -> Value {
    let resp = send(app, "POST", "/api/snacks", Some(token), Some(payload)).await;
    assert_eq!(
        resp.status,
        StatusCode::CREATED,
        "创建零食失败: {}",
        resp.body
    );
    resp.json()
}

fn names(list: &Value) -> Vec<String> {
    list.as_array()
        .expect("应为数组")
        .iter()
        .map(|item| {
            item.get("name")
                .and_then(Value::as_str)
                .expect("name 字段")
                .to_owned()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 认证
// ---------------------------------------------------------------------------

#[tokio::test]
async fn login_success_and_failure() {
    let _guard = test_guard();
    let dir = TempDir::new("login");
    let app = build_app(dir.cfg()).expect("build_app");

    // 登录成功（空库自动初始化的 admin，契约 §1/§3）
    let resp = send(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"username": "admin", "password": "admin123"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    let body = resp.json();
    let token = body["token"].as_str().expect("token");
    assert_eq!(token.len(), 64, "token 应为 32 字节熵的 hex 编码");
    assert_eq!(body["user"]["username"], "admin");
    assert_eq!(body["user"]["is_admin"], true);
    assert!(body["user"]["id"].is_i64());

    // GET /api/auth/me
    let resp = send(&app, "GET", "/api/auth/me", Some(token), None).await;
    assert_eq!(resp.status, StatusCode::OK);
    assert_eq!(resp.json()["username"], "admin");

    // 密码错误 → 401 + 统一错误体
    let resp = send(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"username": "admin", "password": "wrong-password"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
    assert!(!resp.json()["error"].as_str().unwrap_or("").is_empty());

    // 用户不存在 → 401
    let resp = send(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"username": "nobody", "password": "whatever"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);

    // 字段缺失 → 按“失败 401”处理
    let resp = send(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"username": "admin"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);

    // 非法 JSON → 401
    let resp = send_raw(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some("not-json".to_owned()),
    )
    .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_requires_bearer_token_401() {
    let _guard = test_guard();
    let dir = TempDir::new("auth401");
    let app = build_app(dir.cfg()).expect("build_app");

    // 除 POST /api/auth/login 外，所有 /api/* 无 token → 401（契约 §2）
    let protected: &[(&str, &str)] = &[
        ("GET", "/api/snacks"),
        ("POST", "/api/snacks"),
        ("PUT", "/api/snacks/1"),
        ("DELETE", "/api/snacks/1"),
        ("GET", "/api/locations"),
        ("GET", "/api/reminders"),
        ("GET", "/api/users"),
        ("POST", "/api/users"),
        ("DELETE", "/api/users/1"),
        ("GET", "/api/auth/me"),
        ("POST", "/api/auth/logout"),
        ("POST", "/api/auth/change-password"),
        ("GET", "/api/unknown-endpoint"),
    ];
    for (method, uri) in protected {
        let resp = send(&app, method, uri, None, Some(json!({}))).await;
        assert_eq!(
            resp.status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} 无 token 应为 401，实际 {}",
            resp.status
        );
        assert!(
            resp.json().get("error").is_some(),
            "{method} {uri} 错误体应为 {{error}}"
        );
    }

    // 无效 token → 401
    let resp = send(&app, "GET", "/api/snacks", Some("invalid-token"), None).await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);

    // Bearer 后为空 → 401
    let resp = send(&app, "GET", "/api/snacks", Some(""), None).await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);

    // 非 /api 路径不需要 token（静态 fallback，此处目录不存在 → 404 而非 401）
    let resp = send(&app, "GET", "/", None, None).await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn users_endpoints_forbidden_for_non_admin() {
    let _guard = test_guard();
    let dir = TempDir::new("admin403");
    let app = build_app(dir.cfg()).expect("build_app");
    let admin_token = token_of(&app, "admin", "admin123").await;

    // 管理员创建普通用户
    let resp = send(
        &app,
        "POST",
        "/api/users",
        Some(&admin_token),
        Some(json!({"username": "alice", "password": "alice-pass"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CREATED, "{}", resp.body);
    let alice = resp.json();
    let alice_id = alice["id"].as_i64().expect("id");
    assert_eq!(alice["is_admin"], false);
    let alice_token = token_of(&app, "alice", "alice-pass").await;

    // 非管理员访问账户管理接口 → 403（契约 §3）
    let resp = send(&app, "GET", "/api/users", Some(&alice_token), None).await;
    assert_eq!(resp.status, StatusCode::FORBIDDEN);

    let resp = send(
        &app,
        "POST",
        "/api/users",
        Some(&alice_token),
        Some(json!({"username": "mallory", "password": "mallory-pass"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::FORBIDDEN);

    let resp = send(
        &app,
        "DELETE",
        &format!("/api/users/{alice_id}"),
        Some(&alice_token),
        None,
    )
    .await;
    assert_eq!(resp.status, StatusCode::FORBIDDEN);

    // 目标 id 不存在也先鉴权 → 403（而非 404）
    let resp = send(&app, "DELETE", "/api/users/99999", Some(&alice_token), None).await;
    assert_eq!(resp.status, StatusCode::FORBIDDEN);

    // 普通用户仍可访问业务接口
    let resp = send(&app, "GET", "/api/snacks", Some(&alice_token), None).await;
    assert_eq!(resp.status, StatusCode::OK);

    // 管理员可访问
    let resp = send(&app, "GET", "/api/users", Some(&admin_token), None).await;
    assert_eq!(resp.status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// 零食 CRUD 与校验
// ---------------------------------------------------------------------------

#[tokio::test]
async fn snacks_crud_and_validation() {
    let _guard = test_guard();
    let dir = TempDir::new("snacks-crud");
    let app = build_app(dir.cfg()).expect("build_app");
    let token = token_of(&app, "admin", "admin123").await;

    // CREATE → 201，返回完整 Snack
    let payload = json!({
        "name": "薯片", "quantity": 3, "location": "客厅/零食柜",
        "purchase_date": "2026-01-05", "expiry_date": "2027-01-01",
        "notes": "原味", "opened_at": null, "finished_at": null, "status": "in_stock"
    });
    let resp = send(&app, "POST", "/api/snacks", Some(&token), Some(payload)).await;
    assert_eq!(resp.status, StatusCode::CREATED, "{}", resp.body);
    let created = resp.json();
    let id = created["id"].as_i64().expect("id");
    assert_eq!(created["name"], "薯片");
    assert_eq!(created["quantity"], 3);
    assert_eq!(created["status"], "in_stock");
    assert_eq!(created["purchase_date"], "2026-01-05");
    assert!(created["created_at"].is_string());
    assert!(created["updated_at"].is_string());

    // 省略 status/opened_at/finished_at → 默认 in_stock / null（契约 §3）
    let resp = send(
        &app,
        "POST",
        "/api/snacks",
        Some(&token),
        Some(json!({"name": "牛奶", "quantity": 1, "location": "厨房/冰箱"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CREATED, "{}", resp.body);
    let milk = resp.json();
    assert_eq!(milk["status"], "in_stock");
    assert!(milk["opened_at"].is_null());
    assert!(milk["finished_at"].is_null());

    // LIST → 裸数组（契约 §3）
    let resp = send(&app, "GET", "/api/snacks", Some(&token), None).await;
    assert_eq!(resp.status, StatusCode::OK);
    let list = resp.json();
    assert!(list.is_array(), "GET /api/snacks 必须是裸数组");
    assert_eq!(list.as_array().unwrap().len(), 2);

    // UPDATE → 200，全量替换
    let resp = send(
        &app,
        "PUT",
        &format!("/api/snacks/{id}"),
        Some(&token),
        Some(json!({
            "name": "薯片(更新)", "quantity": 10, "location": "客厅/零食柜/第二层",
            "purchase_date": null, "expiry_date": null, "notes": null
        })),
    )
    .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    let updated = resp.json();
    assert_eq!(updated["name"], "薯片(更新)");
    assert_eq!(updated["quantity"], 10);
    assert!(updated["purchase_date"].is_null());

    // UPDATE 不存在 → 404
    let resp = send(
        &app,
        "PUT",
        "/api/snacks/99999",
        Some(&token),
        Some(json!({"name": "x", "quantity": 1, "location": "y"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);

    // DELETE → 204；再次删除 → 404
    let resp = send(
        &app,
        "DELETE",
        &format!("/api/snacks/{id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(resp.status, StatusCode::NO_CONTENT);
    let resp = send(
        &app,
        "DELETE",
        &format!("/api/snacks/{id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);

    // 校验失败 → 400 + {"error"}（缺字段 / quantity<0 / 日期时间格式 / status 非法）
    let bad_cases: Vec<Value> = vec![
        json!({}),                                                   // 缺全部必填
        json!({"quantity": 1, "location": "客厅"}),                  // 缺 name
        json!({"name": "x", "location": "客厅"}),                    // 缺 quantity
        json!({"name": "x", "quantity": 1}),                         // 缺 location
        json!({"name": "  ", "quantity": 1, "location": "客厅"}),    // 空名称
        json!({"name": "x", "quantity": 1, "location": " "}),        // 空位置
        json!({"name": "x", "quantity": -1, "location": "客厅"}),    // 负数量
        json!({"name": "x", "quantity": "abc", "location": "客厅"}), // quantity 类型错误
        json!({"name": "x", "quantity": 1, "location": "客厅", "purchase_date": "2026/01/05"}),
        json!({"name": "x", "quantity": 1, "location": "客厅", "expiry_date": "not-a-date"}),
        json!({"name": "x", "quantity": 1, "location": "客厅", "expiry_date": "2026-02-30"}),
        json!({"name": "x", "quantity": 1, "location": "客厅", "status": "bad-status"}),
        json!({"name": "x", "quantity": 1, "location": "客厅", "opened_at": "2026-01-05T10:00"}),
        json!({"name": "x", "quantity": 1, "location": "客厅", "finished_at": "2026-01-05T10:00"}),
    ];
    for (index, case) in bad_cases.iter().enumerate() {
        let resp = send(
            &app,
            "POST",
            "/api/snacks",
            Some(&token),
            Some(case.clone()),
        )
        .await;
        assert_eq!(
            resp.status,
            StatusCode::BAD_REQUEST,
            "case {index}: {}",
            resp.body
        );
        assert!(resp.json().get("error").is_some(), "case {index} 错误体");
    }

    // 非法 JSON 请求体 → 400
    let resp = send_raw(
        &app,
        "POST",
        "/api/snacks",
        Some(&token),
        Some("nope".to_owned()),
    )
    .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);

    // PUT 同样执行校验（先校验后查存在性）
    let resp = send(
        &app,
        "PUT",
        "/api/snacks/99999",
        Some(&token),
        Some(json!({"name": "x", "quantity": -5, "location": "y"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// 生命周期状态（契约增量 1）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn snacks_lifecycle_status_and_finished_autofill() {
    let _guard = test_guard();
    let dir = TempDir::new("lifecycle");
    let app = build_app(dir.cfg()).expect("build_app");
    let token = token_of(&app, "admin", "admin123").await;

    // status=opened + opened_at 纯日期正常保存
    let resp = send(
        &app,
        "POST",
        "/api/snacks",
        Some(&token),
        Some(json!({
            "name": "酸奶", "quantity": 2, "location": "厨房/冰箱",
            "status": "opened", "opened_at": "2026-03-01"
        })),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CREATED, "{}", resp.body);
    let yogurt = resp.json();
    assert_eq!(yogurt["status"], "opened");
    assert_eq!(yogurt["opened_at"], "2026-03-01");

    // status=finished 未带 finished_at → 服务端自动填充当前本地日期（契约 v2）
    let resp = send(
        &app,
        "POST",
        "/api/snacks",
        Some(&token),
        Some(json!({"name": "饼干", "quantity": 1, "location": "客厅", "status": "finished"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CREATED, "{}", resp.body);
    let biscuit = resp.json();
    assert_eq!(biscuit["status"], "finished");
    let finished_at = biscuit["finished_at"]
        .as_str()
        .expect("自动填充 finished_at");
    assert_eq!(finished_at, local_date(0), "自动填充当前本地日期");
    assert_eq!(finished_at.len(), 10, "应为 YYYY-MM-DD: {finished_at}");

    // status=finished 带显式 finished_at → 保留原值
    let resp = send(
        &app,
        "POST",
        "/api/snacks",
        Some(&token),
        Some(json!({
            "name": "蛋糕", "quantity": 1, "location": "客厅",
            "status": "finished", "finished_at": "2026-03-02"
        })),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CREATED);
    assert_eq!(resp.json()["finished_at"], "2026-03-02");

    // 非法日期格式 → 400（POST 与 PUT；v1 的 datetime 在 v2 必须被拒绝）
    for field in ["opened_at", "finished_at"] {
        for invalid in [
            "2026-03-01T09:30",
            "2026-03-01 09:30",
            "2026-02-30",
            "2026/03/01",
        ] {
            let mut payload = json!({"name": "x", "quantity": 1, "location": "y"});
            payload[field] = json!(invalid);
            let resp = send(
                &app,
                "POST",
                "/api/snacks",
                Some(&token),
                Some(payload.clone()),
            )
            .await;
            assert_eq!(
                resp.status,
                StatusCode::BAD_REQUEST,
                "POST {field}={invalid}: {}",
                resp.body
            );
            assert!(resp.json().get("error").is_some());

            let resp = send(
                &app,
                "PUT",
                &format!("/api/snacks/{}", yogurt["id"].as_i64().unwrap()),
                Some(&token),
                Some(payload),
            )
            .await;
            assert_eq!(
                resp.status,
                StatusCode::BAD_REQUEST,
                "PUT {field}={invalid}: {}",
                resp.body
            );
        }
    }

    // 非法 status 枚举 → 400（POST 与 PUT）
    let resp = send(
        &app,
        "POST",
        "/api/snacks",
        Some(&token),
        Some(json!({"name": "x", "quantity": 1, "location": "y", "status": "eaten"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);

    // PUT 全量替换：省略 status → 回到默认 in_stock
    let resp = send(
        &app,
        "PUT",
        &format!("/api/snacks/{}", yogurt["id"].as_i64().unwrap()),
        Some(&token),
        Some(json!({"name": "酸奶", "quantity": 2, "location": "厨房/冰箱"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    let updated = resp.json();
    assert_eq!(updated["status"], "in_stock");
    assert!(updated["opened_at"].is_null());
}

// ---------------------------------------------------------------------------
// 生产日期 / 保质期 / 过期日期：填两项自动算第三项
// ---------------------------------------------------------------------------

#[tokio::test]
async fn production_shelf_and_expiry_are_derived() {
    let _guard = test_guard();
    let dir = TempDir::new("production-shelf");
    let app = build_app(dir.cfg()).expect("build_app");
    let token = token_of(&app, "admin", "admin123").await;

    // 生产日期 + 保质期 → 过期日期
    let resp = send(
        &app,
        "POST",
        "/api/snacks",
        Some(&token),
        Some(json!({
            "name": "牛奶", "quantity": 1, "location": "厨房/冰箱",
            "production_date": "2026-01-01", "shelf_life_days": 30
        })),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CREATED, "{}", resp.body);
    let milk = resp.json();
    assert_eq!(milk["production_date"], "2026-01-01");
    assert_eq!(milk["shelf_life_days"], 30);
    assert_eq!(milk["shelf_life_unit"], "day");
    assert_eq!(milk["expiry_date"], "2026-01-31");

    // 生产日期 + 过期日期 → 保质期
    let resp = send(
        &app,
        "POST",
        "/api/snacks",
        Some(&token),
        Some(json!({
            "name": "面包", "quantity": 1, "location": "厨房",
            "production_date": "2026-03-01", "expiry_date": "2026-04-30"
        })),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CREATED, "{}", resp.body);
    let bread = resp.json();
    assert_eq!(bread["shelf_life_days"], 60);
    assert_eq!(bread["expiry_date"], "2026-04-30");

    // 保质期 + 过期日期 → 生产日期
    let resp = send(
        &app,
        "POST",
        "/api/snacks",
        Some(&token),
        Some(json!({
            "name": "饼干", "quantity": 1, "location": "客厅",
            "shelf_life_days": 10, "expiry_date": "2026-01-11"
        })),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CREATED, "{}", resp.body);
    let biscuit = resp.json();
    assert_eq!(biscuit["production_date"], "2026-01-01");
    assert_eq!(biscuit["shelf_life_days"], 10);

    // 保质期单位可调整：6 个月 = 180 天，单位随记录保存
    let resp = send(
        &app,
        "POST",
        "/api/snacks",
        Some(&token),
        Some(json!({
            "name": "按月", "quantity": 1, "location": "厨房",
            "production_date": "2026-01-01", "shelf_life_days": 180,
            "shelf_life_unit": "month"
        })),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CREATED, "{}", resp.body);
    let monthly = resp.json();
    assert_eq!(monthly["shelf_life_days"], 180);
    assert_eq!(monthly["shelf_life_unit"], "month");
    assert_eq!(monthly["expiry_date"], "2026-06-30");

    // 三项齐全且自洽 → 保持原值
    let resp = send(
        &app,
        "POST",
        "/api/snacks",
        Some(&token),
        Some(json!({
            "name": "自洽", "quantity": 1, "location": "客厅",
            "production_date": "2026-01-01", "shelf_life_days": 30,
            "expiry_date": "2026-01-31"
        })),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CREATED, "{}", resp.body);

    // 三项齐全但不自洽 → 400（不静默改写客户端明确给出的值）
    let resp = send(
        &app,
        "POST",
        "/api/snacks",
        Some(&token),
        Some(json!({
            "name": "矛盾", "quantity": 1, "location": "客厅",
            "production_date": "2026-01-01", "shelf_life_days": 30,
            "expiry_date": "2026-02-01"
        })),
    )
    .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST, "{}", resp.body);
    assert!(
        resp.body.contains("不一致"),
        "错误信息应说明三项不一致: {}",
        resp.body
    );

    // 过期早于生产 → 400
    let resp = send(
        &app,
        "POST",
        "/api/snacks",
        Some(&token),
        Some(json!({
            "name": "倒挂", "quantity": 1, "location": "客厅",
            "production_date": "2026-02-01", "expiry_date": "2026-01-01"
        })),
    )
    .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST, "{}", resp.body);

    // 保质期越界 / 生产日期带时间 → 400
    for payload in [
        json!({"name": "负数", "quantity": 1, "location": "客厅", "shelf_life_days": -1}),
        json!({"name": "过长", "quantity": 1, "location": "客厅", "shelf_life_days": 36_501}),
        json!({"name": "时间", "quantity": 1, "location": "客厅", "production_date": "2026-01-01T08:00"}),
        json!({"name": "单位", "quantity": 1, "location": "客厅", "shelf_life_days": 30, "shelf_life_unit": "week"}),
    ] {
        let resp = send(&app, "POST", "/api/snacks", Some(&token), Some(payload)).await;
        assert_eq!(resp.status, StatusCode::BAD_REQUEST, "{}", resp.body);
        assert!(resp.json().get("error").is_some());
    }

    // PUT 同样自动计算：改生产日期，保质期不变则过期日期顺延
    let updated = json!({
        "name": "牛奶", "quantity": 1, "location": "厨房/冰箱",
        "production_date": "2026-02-01", "shelf_life_days": 30
    });
    let resp = send(
        &app,
        "PUT",
        &format!("/api/snacks/{}", milk["id"].as_i64().unwrap()),
        Some(&token),
        Some(updated),
    )
    .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    assert_eq!(resp.json()["expiry_date"], "2026-03-03");

    // 提醒接口返回的 Snack 字段同样包含生产日期/保质期
    let resp = send(&app, "GET", "/api/reminders", Some(&token), None).await;
    assert_eq!(resp.status, StatusCode::OK);
    let reminders = resp.json();
    let item = reminders
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "牛奶")
        .expect("牛奶应在提醒中");
    assert_eq!(item["production_date"], "2026-02-01");
    assert_eq!(item["shelf_life_days"], 30);
    assert_eq!(item["shelf_life_unit"], "day");
}

#[tokio::test]
async fn legacy_database_gets_production_and_shelf_columns() {
    let _guard = test_guard();
    let dir = TempDir::new("legacy-columns");
    let cfg = dir.cfg();

    // 模拟 v2 存量库：snacks 表没有 production_date / shelf_life_days。
    {
        let conn = rusqlite::Connection::open(&cfg.db_path).expect("open sqlite");
        conn.execute_batch(
            "CREATE TABLE snacks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                quantity INTEGER NOT NULL DEFAULT 1,
                location TEXT NOT NULL,
                purchase_date TEXT,
                expiry_date TEXT,
                opened_at TEXT,
                finished_at TEXT,
                status TEXT NOT NULL DEFAULT 'in_stock',
                notes TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO snacks (name, quantity, location, created_at, updated_at)              VALUES ('旧库零食', 1, '客厅', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    }

    let app = build_app(cfg).expect("build_app migrates columns");
    let token = token_of(&app, "admin", "admin123").await;
    let resp = send(&app, "GET", "/api/snacks", Some(&token), None).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    let items = resp.json();
    let old = items
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "旧库零食")
        .expect("旧库记录");
    assert!(old["production_date"].is_null());
    assert!(old["shelf_life_days"].is_null());
    assert_eq!(old["shelf_life_unit"], "day");

    // 旧库行可通过 PUT 写入新字段并自动推导
    let id = old["id"].as_i64().unwrap();
    let resp = send(
        &app,
        "PUT",
        &format!("/api/snacks/{id}"),
        Some(&token),
        Some(json!({
            "name": "旧库零食", "quantity": 1, "location": "客厅",
            "production_date": "2026-01-01", "shelf_life_days": 7
        })),
    )
    .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    assert_eq!(resp.json()["expiry_date"], "2026-01-08");
    assert_eq!(resp.json()["shelf_life_unit"], "day");
}

// ---------------------------------------------------------------------------
// v1 → v2 存量迁移（启动时幂等规范化 opened_at/finished_at）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn legacy_datetime_values_are_migrated_on_startup() {
    let _guard = test_guard();
    let dir = TempDir::new("legacy-migration");
    let cfg = dir.cfg();

    // 先启动一次完成建表，然后关闭连接（Router drop 释放 SQLite 连接）。
    let app = build_app(cfg.clone()).expect("build_app");
    drop(app);

    // 模拟 v1 存量数据：合法 datetime、旧格式、非法值。
    {
        let conn = rusqlite::Connection::open(&cfg.db_path).expect("open sqlite");
        conn.execute(
            "INSERT INTO snacks (name, quantity, location, opened_at, finished_at, status, \
             created_at, updated_at) VALUES \
             ('旧格式', 1, '厨房/冰箱', '2026-03-01T09:30', '2026-03-02 18:00', 'finished', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO snacks (name, quantity, location, opened_at, finished_at, status, \
             created_at, updated_at) VALUES \
             ('非法值', 1, '客厅', 'not-a-date', '2026-02-30T10:00', 'opened', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    }

    // 再次启动 → 迁移执行；接口只返回纯日期，非法值变 null。
    let app = build_app(cfg.clone()).expect("rebuild build_app");
    let token = token_of(&app, "admin", "admin123").await;
    let resp = send(&app, "GET", "/api/snacks", Some(&token), None).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    let list = resp.json();
    let items = list.as_array().expect("裸数组");
    assert_eq!(items.len(), 2);
    let migrated = items
        .iter()
        .find(|item| item["name"] == "旧格式")
        .expect("旧格式记录");
    assert_eq!(migrated["opened_at"], "2026-03-01");
    assert_eq!(migrated["finished_at"], "2026-03-02");
    assert_eq!(migrated["status"], "finished");
    let invalid = items
        .iter()
        .find(|item| item["name"] == "非法值")
        .expect("非法值记录");
    assert!(invalid["opened_at"].is_null(), "非法 opened_at 应置 null");
    assert!(
        invalid["finished_at"].is_null(),
        "非法 finished_at 应置 null"
    );
    drop(app);

    // 幂等：第三次启动不改变已规范化的值。
    let app = build_app(cfg).expect("third build_app");
    let token = token_of(&app, "admin", "admin123").await;
    let resp = send(&app, "GET", "/api/snacks", Some(&token), None).await;
    let again = resp.json();
    let migrated_again = again
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "旧格式")
        .unwrap();
    assert_eq!(migrated_again["opened_at"], "2026-03-01");
    assert_eq!(migrated_again["finished_at"], "2026-03-02");
}

// ---------------------------------------------------------------------------
// 列表过滤（q / location / expiry_status / status）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn snacks_list_filters() {
    let _guard = test_guard();
    let dir = TempDir::new("snacks-filters");
    let app = build_app(dir.cfg()).expect("build_app");
    let token = token_of(&app, "admin", "admin123").await;

    // 1 薯片：过期(expired)，notes 含“辣味”
    let mut chips = snack("薯片", "客厅/零食柜", Some(&local_date(-1)));
    chips["notes"] = json!("辣味");
    create_snack(&app, &token, chips).await;
    // 2 牛奶：expiring（+3 天），生命周期 opened
    let mut milk = snack("牛奶", "厨房/冰箱", Some(&local_date(3)));
    milk["status"] = json!("opened");
    milk["opened_at"] = json!(local_date(-1));
    create_snack(&app, &token, milk).await;
    // 3 饼干：ok（+30 天），生命周期 finished
    let mut biscuit = snack("饼干", "客厅/零食柜", Some(&local_date(30)));
    biscuit["status"] = json!("finished");
    create_snack(&app, &token, biscuit).await;
    // 4 面条：无过期日期，in_stock
    create_snack(&app, &token, snack("面条", "阳台/储物箱", None)).await;

    // 缺省 → 全部（裸数组，最新创建优先）
    let resp = send(&app, "GET", "/api/snacks", Some(&token), None).await;
    assert_eq!(names(&resp.json()), vec!["面条", "饼干", "牛奶", "薯片"]);

    // q 匹配 name
    let resp = send(&app, "GET", "/api/snacks?q=薯", Some(&token), None).await;
    assert_eq!(names(&resp.json()), vec!["薯片"]);
    // q 匹配 notes
    let resp = send(&app, "GET", "/api/snacks?q=辣味", Some(&token), None).await;
    assert_eq!(names(&resp.json()), vec!["薯片"]);
    // q 无匹配
    let resp = send(&app, "GET", "/api/snacks?q=不存在", Some(&token), None).await;
    assert_eq!(names(&resp.json()), Vec::<String>::new());

    // location 精确匹配
    let resp = send(
        &app,
        "GET",
        "/api/snacks?location=%E5%AE%A2%E5%8E%85%2F%E9%9B%B6%E9%A3%9F%E6%9F%9C",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(names(&resp.json()), vec!["饼干", "薯片"]);

    // expiry_status 过期维度（契约改名后的参数）
    let resp = send(
        &app,
        "GET",
        "/api/snacks?expiry_status=expired",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(names(&resp.json()), vec!["薯片"]);
    let resp = send(
        &app,
        "GET",
        "/api/snacks?expiry_status=expiring",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(names(&resp.json()), vec!["牛奶"]);
    let resp = send(
        &app,
        "GET",
        "/api/snacks?expiry_status=ok",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(names(&resp.json()), vec!["面条", "饼干"]);
    let resp = send(
        &app,
        "GET",
        "/api/snacks?expiry_status=all",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(names(&resp.json()).len(), 4);

    // status 生命周期维度
    let resp = send(
        &app,
        "GET",
        "/api/snacks?status=in_stock",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(names(&resp.json()), vec!["面条", "薯片"]);
    let resp = send(&app, "GET", "/api/snacks?status=opened", Some(&token), None).await;
    assert_eq!(names(&resp.json()), vec!["牛奶"]);
    let resp = send(
        &app,
        "GET",
        "/api/snacks?status=finished",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(names(&resp.json()), vec!["饼干"]);

    // 两个维度可组合
    let resp = send(
        &app,
        "GET",
        "/api/snacks?expiry_status=ok&status=finished",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(names(&resp.json()), vec!["饼干"]);

    // 非法枚举 → 400
    let resp = send(
        &app,
        "GET",
        "/api/snacks?expiry_status=weird",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
    // 改名后的破坏性变更：旧值 expired 现在是非法 status → 400
    let resp = send(
        &app,
        "GET",
        "/api/snacks?status=expired",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);

    // GET /api/locations：去重 + 字典序
    let resp = send(&app, "GET", "/api/locations", Some(&token), None).await;
    assert_eq!(resp.status, StatusCode::OK);
    let locations = resp.json();
    let mut sorted = locations.as_array().unwrap().clone();
    sorted.sort_by(|a, b| a.as_str().unwrap().cmp(b.as_str().unwrap()));
    assert_eq!(*locations.as_array().unwrap(), sorted, "应按字典序");
    let values: Vec<&str> = locations
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(values, vec!["厨房/冰箱", "客厅/零食柜", "阳台/储物箱"]);
    assert_eq!(values.len(), 3, "应去重");

    // v2：q 子串过滤（缺省/空串仍返回全部）
    for uri in ["/api/locations?q=", "/api/locations?q=%20"] {
        let resp = send(&app, "GET", uri, Some(&token), None).await;
        assert_eq!(resp.status, StatusCode::OK, "{uri}: {}", resp.body);
        assert_eq!(resp.json().as_array().unwrap().len(), 3, "{uri}");
    }
    let resp = send(
        &app,
        "GET",
        "/api/locations?q=%E5%AE%A2",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    assert_eq!(resp.json(), json!(["客厅/零食柜"]));
    let resp = send(
        &app,
        "GET",
        "/api/locations?q=%E5%8E%A8",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(resp.json(), json!(["厨房/冰箱"]));
    let resp = send(
        &app,
        "GET",
        "/api/locations?q=%E4%B8%8D%E5%AD%98%E5%9C%A8",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(resp.json(), json!([]));
}

// ---------------------------------------------------------------------------
// 搜索：LIKE 通配符转义（ESCAPE 子句回归）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn snacks_search_escapes_like_wildcards() {
    let _guard = test_guard();
    let dir = TempDir::new("snacks-like-escape");
    let app = build_app(dir.cfg()).expect("build_app");
    let token = token_of(&app, "admin", "admin123").await;

    // 名称含字面量 `%`：`100%` 与会通配符误命中的 `100袋`
    create_snack(&app, &token, snack("能量棒100%", "客厅/零食柜", None)).await;
    create_snack(&app, &token, snack("能量棒100袋", "客厅/零食柜", None)).await;
    // 名称含字面量 `_`：`a_b` 与会被 `_` 通配符误命中的 `aXb`
    create_snack(&app, &token, snack("薯条a_b口味", "厨房/冰箱", None)).await;
    create_snack(&app, &token, snack("薯条aXb口味", "厨房/冰箱", None)).await;
    // 位置含字面量 `%`：`折扣100%` 与会被 `%` 通配符误命中的 `折扣100袋`
    create_snack(&app, &token, snack("位置A", "折扣100%", None)).await;
    create_snack(&app, &token, snack("位置B", "折扣100袋", None)).await;

    // `100%`：只命中包含字面量 `100%` 的名称（`%` 不再是通配符）
    let resp = send(&app, "GET", "/api/snacks?q=100%25", Some(&token), None).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    assert_eq!(names(&resp.json()), vec!["能量棒100%"]);

    // `a_b`：只命中字面量 `a_b`，不误命中 `aXb`（`_` 不再是单字符通配符）
    let resp = send(&app, "GET", "/api/snacks?q=a_b", Some(&token), None).await;
    assert_eq!(names(&resp.json()), vec!["薯条a_b口味"]);

    // 普通子串搜索不受转义影响：`100` 仍同时命中两个名称（最新创建优先）
    let resp = send(&app, "GET", "/api/snacks?q=100", Some(&token), None).await;
    assert_eq!(names(&resp.json()), vec!["能量棒100袋", "能量棒100%"]);

    // locations 的 q 同样执行 LIKE 转义：`100%` 不得命中 `折扣100袋`
    let resp = send(&app, "GET", "/api/locations?q=100%25", Some(&token), None).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    assert_eq!(resp.json(), json!(["折扣100%"]));
    let resp = send(&app, "GET", "/api/locations?q=100", Some(&token), None).await;
    assert_eq!(resp.json(), json!(["折扣100%", "折扣100袋"]));
}

// ---------------------------------------------------------------------------
// 过期提醒（level 分级、days_left、排序、排除 finished）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reminders_level_order_and_finished_exclusion() {
    let _guard = test_guard();
    let dir = TempDir::new("reminders");
    let app = build_app(dir.cfg()).expect("build_app");
    let token = token_of(&app, "admin", "admin123").await;

    // 近期数据：只在 days_left ≤ 7 的窗口内
    create_snack(
        &app,
        &token,
        snack("过期奶", "厨房/冰箱", Some(&local_date(-3))),
    )
    .await;
    create_snack(
        &app,
        &token,
        snack("今天到期", "厨房/冰箱", Some(&local_date(0))),
    )
    .await;
    let mut soon = snack("两天", "厨房/冰箱", Some(&local_date(2)));
    soon["status"] = json!("opened");
    soon["opened_at"] = json!(local_date(0));
    create_snack(&app, &token, soon).await;
    create_snack(
        &app,
        &token,
        snack("五天", "客厅/零食柜", Some(&local_date(5))),
    )
    .await;
    create_snack(
        &app,
        &token,
        snack("七天", "客厅/零食柜", Some(&local_date(7))),
    )
    .await;
    // 应被排除：远期 / 无过期日期 / 已吃完
    create_snack(
        &app,
        &token,
        snack("远期", "客厅/零食柜", Some(&local_date(30))),
    )
    .await;
    create_snack(&app, &token, snack("无日期", "阳台/储物箱", None)).await;
    let mut done = snack("已吃完", "厨房/冰箱", Some(&local_date(1)));
    done["status"] = json!("finished");
    done["finished_at"] = json!(local_date(1));
    create_snack(&app, &token, done).await;

    let resp = send(&app, "GET", "/api/reminders", Some(&token), None).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    let reminders = resp.json();
    let items = reminders.as_array().expect("提醒应为数组");
    assert_eq!(items.len(), 5, "应排除远期/无过期日期/已吃完: {reminders}");

    // 按 expiry_date 升序
    assert_eq!(
        names(&reminders),
        vec!["过期奶", "今天到期", "两天", "五天", "七天"]
    );

    // 分级字段为 level（非 status），值与 days_left 计算正确
    let expected_levels = ["expired", "critical", "critical", "warning", "warning"];
    let expected_days: [i64; 5] = [-3, 0, 2, 5, 7];
    for (index, (item, (level, days))) in items
        .iter()
        .zip(expected_levels.into_iter().zip(expected_days))
        .enumerate()
    {
        assert_eq!(item["level"], level, "item {index}");
        assert_eq!(item["days_left"].as_i64(), Some(days), "item {index}");
    }

    // 元素 = Snack 全字段（含新字段 status/opened_at/finished_at）+ days_left + level
    let first = &items[0];
    for key in [
        "id",
        "name",
        "quantity",
        "location",
        "purchase_date",
        "expiry_date",
        "opened_at",
        "finished_at",
        "status",
        "notes",
        "created_at",
        "updated_at",
        "days_left",
        "level",
    ] {
        assert!(first.get(key).is_some(), "提醒元素缺少字段 {key}");
    }
    // level 与 Snack 的生命周期 status 不混用
    assert_eq!(first["status"], "in_stock");
    assert_eq!(first["level"], "expired");
    // 生命周期 opened 的零食也出现在提醒中（只有 finished 被排除）
    assert_eq!(items[2]["status"], "opened");
}

// ---------------------------------------------------------------------------
// 账户管理
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_user_management() {
    let _guard = test_guard();
    let dir = TempDir::new("users");
    let app = build_app(dir.cfg()).expect("build_app");
    let admin = login(&app, "admin", "admin123").await;
    let admin_token = admin["token"].as_str().unwrap().to_owned();
    let admin_id = admin["user"]["id"].as_i64().expect("admin id");

    // 创建用户 → 201，is_admin=false，含 created_at
    let resp = send(
        &app,
        "POST",
        "/api/users",
        Some(&admin_token),
        Some(json!({"username": "bob", "password": "bob-secret"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CREATED, "{}", resp.body);
    let bob = resp.json();
    let bob_id = bob["id"].as_i64().expect("id");
    assert_eq!(bob["username"], "bob");
    assert_eq!(bob["is_admin"], false);
    assert!(bob["created_at"].is_string());

    // 用户名重复 → 409
    let resp = send(
        &app,
        "POST",
        "/api/users",
        Some(&admin_token),
        Some(json!({"username": "bob", "password": "another-secret"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CONFLICT);
    assert!(resp.json().get("error").is_some());

    // 创建校验 → 400
    for payload in [
        json!({}),
        json!({"username": "carol"}),
        json!({"password": "carol-pass"}),
        json!({"username": " ", "password": "carol-pass"}),
        json!({"username": "carol", "password": ""}),
    ] {
        let resp = send(
            &app,
            "POST",
            "/api/users",
            Some(&admin_token),
            Some(payload),
        )
        .await;
        assert_eq!(resp.status, StatusCode::BAD_REQUEST, "{}", resp.body);
    }

    // 用户列表包含 admin 与 bob
    let resp = send(&app, "GET", "/api/users", Some(&admin_token), None).await;
    let users = resp.json();
    let items = users.as_array().expect("数组");
    assert!(items.len() >= 2);
    assert_eq!(items[0]["username"], "admin");
    assert_eq!(items[0]["is_admin"], true);
    assert!(items.iter().any(|u| u["username"] == "bob"));

    // 删除自己 → 400，文案必须与契约完全一致
    let resp = send(
        &app,
        "DELETE",
        &format!("/api/users/{admin_id}"),
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
    assert_eq!(resp.json()["error"], "不能删除当前登录账户");

    // bob 删除前先登录（用于验证 sessions 级联删除）
    let bob_token = token_of(&app, "bob", "bob-secret").await;
    let resp = send(&app, "GET", "/api/auth/me", Some(&bob_token), None).await;
    assert_eq!(resp.status, StatusCode::OK);

    // 管理员删除 bob → 204；其会话立即失效（级联）
    let resp = send(
        &app,
        "DELETE",
        &format!("/api/users/{bob_id}"),
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(resp.status, StatusCode::NO_CONTENT);
    let resp = send(&app, "GET", "/api/auth/me", Some(&bob_token), None).await;
    assert_eq!(
        resp.status,
        StatusCode::UNAUTHORIZED,
        "删除用户应级联删除 sessions"
    );

    // 再次删除 → 404；不存在的 id → 404
    let resp = send(
        &app,
        "DELETE",
        &format!("/api/users/{bob_id}"),
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
    let resp = send(&app, "DELETE", "/api/users/99999", Some(&admin_token), None).await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn change_password_flow() {
    let _guard = test_guard();
    let dir = TempDir::new("change-password");
    let app = build_app(dir.cfg()).expect("build_app");
    let admin_token = token_of(&app, "admin", "admin123").await;

    // 新密码 <6 → 400
    let resp = send(
        &app,
        "POST",
        "/api/auth/change-password",
        Some(&admin_token),
        Some(json!({"old_password": "admin123", "new_password": "12345"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);

    // 原密码错误 → 400
    let resp = send(
        &app,
        "POST",
        "/api/auth/change-password",
        Some(&admin_token),
        Some(json!({"old_password": "wrong-old", "new_password": "new-secret-1"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);

    // 缺字段 → 400
    let resp = send(
        &app,
        "POST",
        "/api/auth/change-password",
        Some(&admin_token),
        Some(json!({})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);

    // 成功 → 204；旧密码失效、新密码可登录
    let resp = send(
        &app,
        "POST",
        "/api/auth/change-password",
        Some(&admin_token),
        Some(json!({"old_password": "admin123", "new_password": "new-secret-1"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::NO_CONTENT, "{}", resp.body);

    let resp = send(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"username": "admin", "password": "admin123"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
    let _ = token_of(&app, "admin", "new-secret-1").await;

    // 普通用户也可修改自己的密码
    let resp = send(
        &app,
        "POST",
        "/api/users",
        Some(&token_of(&app, "admin", "new-secret-1").await),
        Some(json!({"username": "dave", "password": "dave-pass"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CREATED);
    let dave_token = token_of(&app, "dave", "dave-pass").await;
    let resp = send(
        &app,
        "POST",
        "/api/auth/change-password",
        Some(&dave_token),
        Some(json!({"old_password": "dave-pass", "new_password": "dave-new-pass"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::NO_CONTENT);
    let _ = token_of(&app, "dave", "dave-new-pass").await;
    let resp = send(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"username": "dave", "password": "dave-pass"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_invalidates_session() {
    let _guard = test_guard();
    let dir = TempDir::new("logout");
    let app = build_app(dir.cfg()).expect("build_app");
    let token = token_of(&app, "admin", "admin123").await;

    let resp = send(&app, "GET", "/api/auth/me", Some(&token), None).await;
    assert_eq!(resp.status, StatusCode::OK);

    // 登出 → 204
    let resp = send(&app, "POST", "/api/auth/logout", Some(&token), None).await;
    assert_eq!(resp.status, StatusCode::NO_CONTENT, "{}", resp.body);

    // token 已失效 → 401
    let resp = send(&app, "GET", "/api/auth/me", Some(&token), None).await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// admin 自动初始化（契约增量 2）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_auto_init_on_empty_database() {
    let _guard = test_guard();
    let dir = TempDir::new("admin-init-empty");
    let app = build_app(dir.cfg()).expect("build_app");

    // 空库启动 → admin 可用默认密码登录且 is_admin=true
    let body = login(&app, "admin", "admin123").await;
    assert_eq!(body["user"]["username"], "admin");
    assert_eq!(body["user"]["is_admin"], true);

    let token = body["token"].as_str().unwrap();
    let resp = send(&app, "GET", "/api/users", Some(token), None).await;
    let users = resp.json();
    let items = users.as_array().unwrap();
    assert_eq!(items.len(), 1, "空库应只初始化一个管理员");
    assert_eq!(items[0]["username"], "admin");
    assert_eq!(items[0]["is_admin"], true);
    assert!(items[0]["created_at"].is_string());
}

#[tokio::test]
async fn admin_auto_init_preserves_changed_password() {
    let _guard = test_guard();
    let dir = TempDir::new("admin-init-restart");
    let app = build_app(dir.cfg()).expect("build_app");

    // 用户先改密
    let token = token_of(&app, "admin", "admin123").await;
    let resp = send(
        &app,
        "POST",
        "/api/auth/change-password",
        Some(&token),
        Some(json!({"old_password": "admin123", "new_password": "reboot-proof-1"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::NO_CONTENT);

    // “重启”：同一数据库重新执行初始化逻辑（幂等铁律：绝不覆盖已有用户密码）
    let restarted = build_app(dir.cfg()).expect("重启 build_app");

    // 用户改过的密码仍有效
    let resp = send(
        &restarted,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"username": "admin", "password": "reboot-proof-1"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);

    // 默认密码不再有效（未被重置）
    let resp = send(
        &restarted,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"username": "admin", "password": "admin123"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_auto_init_respects_toml_overrides() {
    let _guard = test_guard();
    // 全局锁保证写入 TOML 期间没有其他测试在启动应用
    let dir = TempDir::new("admin-init-toml");
    let toml_content = r#"
host = "0.0.0.0"
port = 3000
admin_username = "root"
admin_password = "root-secret-1"
"#;
    std::fs::write(dir.path().join("snack-warehouse.toml"), toml_content).unwrap();
    let app = build_app(dir.cfg()).expect("build_app");

    // TOML 中设置的管理员可登录
    let body = login(&app, "root", "root-secret-1").await;
    assert_eq!(body["user"]["username"], "root");
    assert_eq!(body["user"]["is_admin"], true);

    // 默认 admin 未被创建
    let resp = send(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"username": "admin", "password": "admin123"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// 静态托管与 SPA fallback（契约 §0）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn static_spa_fallback_and_graceful_missing_dir() {
    let _guard = test_guard();

    // (a) 静态目录不存在（build.sh 尚未运行）→ 优雅 404，不 panic、不 401
    let missing = TempDir::new("static-missing");
    let app = build_app(missing.cfg()).expect("build_app");
    let resp = send(&app, "GET", "/", None, None).await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
    let resp = send(&app, "GET", "/reminders", None, None).await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);

    // (b) 静态目录存在
    let dir = TempDir::new("static-present");
    let cfg = dir.cfg();
    std::fs::create_dir_all(cfg.static_dir.join("assets")).expect("创建静态目录");
    std::fs::write(
        cfg.static_dir.join("index.html"),
        "<html>SPA-INDEX-MARKER</html>",
    )
    .expect("写 index");
    std::fs::write(
        cfg.static_dir.join("assets").join("app.js"),
        "console.log(1);",
    )
    .expect("写 js");
    // 静态目录之外的敏感文件（用于目录穿越断言）
    std::fs::write(dir.path().join("SECRET.txt"), "secret-source-marker").expect("写 secret");
    let app = build_app(cfg).expect("build_app");

    // GET / → index.html，无需登录，Content-Type 正确
    let resp = send(&app, "GET", "/", None, None).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    assert!(resp.body.contains("SPA-INDEX-MARKER"));
    assert!(
        resp.content_type
            .as_deref()
            .unwrap_or_default()
            .starts_with("text/html"),
        "content-type: {:?}",
        resp.content_type
    );

    // 未知前端路由 → SPA fallback 到 index.html
    let resp = send(&app, "GET", "/reminders", None, None).await;
    assert_eq!(resp.status, StatusCode::OK);
    assert!(resp.body.contains("SPA-INDEX-MARKER"));

    // 不存在的构建产物文件 → 同样回退 index.html
    let resp = send(&app, "GET", "/main-abc123.js", None, None).await;
    assert_eq!(resp.status, StatusCode::OK);
    assert!(resp.body.contains("SPA-INDEX-MARKER"));

    // 存在的文件 → 原样返回 + 正确 MIME
    let resp = send(&app, "GET", "/assets/app.js", None, None).await;
    assert_eq!(resp.status, StatusCode::OK);
    assert_eq!(resp.body, "console.log(1);");
    assert!(
        resp.content_type
            .as_deref()
            .unwrap_or_default()
            .contains("javascript"),
        "content-type: {:?}",
        resp.content_type
    );

    // 目录穿越 → 不得读取静态目录之外的文件
    for uri in [
        "/../SECRET.txt",
        "/%2e%2e/SECRET.txt",
        "/assets/../../SECRET.txt",
    ] {
        let resp = send(&app, "GET", uri, None, None).await;
        assert!(
            !resp.body.contains("secret-source-marker"),
            "{uri} 泄漏了目录外文件"
        );
        assert!(resp.status == StatusCode::OK || resp.status == StatusCode::NOT_FOUND);
    }

    // /api/* 优先于静态路由：无 token → 401；带 token 的未知接口 → 404 JSON
    let resp = send(&app, "GET", "/api/unknown", None, None).await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
    let admin_token = token_of(&app, "admin", "admin123").await;
    let resp = send(&app, "GET", "/api/unknown", Some(&admin_token), None).await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
    assert!(resp.json().get("error").is_some());
}

#[tokio::test]
async fn api_prefix_boundaries_and_method_not_allowed_json() {
    let _guard = test_guard();
    let dir = TempDir::new("api-boundary");
    let app = build_app(dir.cfg()).expect("build_app");
    let admin_token = token_of(&app, "admin", "admin123").await;

    // `/apixxx` 不是 API 路径：无 token 也必须走静态 fallback（而不是 401）。
    let resp = send(&app, "GET", "/apixxx", None, None).await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
    assert!(!resp.body.contains("认证令牌"), "{}", resp.body);

    // 普通用户访问 `/api/usersfoo`：不是账户管理接口，应为 404 JSON 而非 403。
    let resp = send(
        &app,
        "POST",
        "/api/users",
        Some(&admin_token),
        Some(json!({"username": "carol", "password": "carol-pass"})),
    )
    .await;
    assert_eq!(resp.status, StatusCode::CREATED, "{}", resp.body);
    let carol_token = token_of(&app, "carol", "carol-pass").await;
    let resp = send(&app, "GET", "/api/usersfoo", Some(&carol_token), None).await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND, "{}", resp.body);
    assert!(resp.json().get("error").is_some(), "{}", resp.body);

    // 路径存在但方法不支持 → 405，且仍为统一 JSON 错误体。
    let resp = send(&app, "DELETE", "/api/auth/login", Some(&admin_token), None).await;
    assert_eq!(resp.status, StatusCode::METHOD_NOT_ALLOWED, "{}", resp.body);
    assert!(resp.json().get("error").is_some(), "{}", resp.body);
}
