# 零食仓库管理平台 — 集成契约（父级决策，子agent必须遵守）

## 0. 总体架构（不可更改的约束）

- **单体部署，无独立中间件**：不使用 Nginx / API 网关 / 独立认证服务。Rust 后端同时承担静态资源托管，Angular 编译产物直接由 Rust 进程提供服务。
- **技术栈**：后端 Rust（推荐 axum + tokio + rusqlite(bundled)），数据库 SQLite，前端 Angular（standalone components，无 NgRx）。
- **仓库布局（目录所有权，见各子任务）**：
  ```
  snack-warehouse-management/
  ├── CONTRACT.md          # 本文件（父级所有，子agent只读）
  ├── README.md            # 集成阶段所有者编写
  ├── build.sh             # 集成阶段所有者编写（前端构建+拷贝+后端构建）
  ├── .gitignore           # 父级所有，子agent不得修改
  ├── backend/             # 后端 lane 独占
  └── frontend/            # 前端 lane 独占
  ```
- **静态资源托管规则**：
  - 后端从 `backend/static/` 目录提供静态文件（目录可由环境变量 `STATIC_DIR` 覆盖，默认 `backend/static`）。
  - `/api/*` 路径永远优先于静态路由；非 `/api` 且磁盘上无对应文件的路径一律回退到 `index.html`（SPA fallback）。
  - `backend/static/` 及其内容不提交到 git（.gitignore 已忽略），由 build.sh 生成。

## 1. 数据模型（SQLite）

默认数据库文件：`data/app.db`（可用环境变量 `SNACK_DB_PATH` 覆盖；`data/` 已被 .gitignore 忽略）。首次启动自动建表；若 `users` 表为空，自动创建默认管理员 `admin` / `admin123`（README 必须提示登录后立即修改密码）。

### users
| 列 | 类型 | 说明 |
|---|---|---|
| id | INTEGER PK AUTOINCREMENT | |
| username | TEXT UNIQUE NOT NULL | 登录名 |
| password_hash | TEXT NOT NULL | argon2 或 bcrypt 哈希，禁止明文 |
| is_admin | INTEGER NOT NULL DEFAULT 0 | 0/1 |
| created_at | TEXT NOT NULL | RFC3339 UTC |

### sessions
| 列 | 类型 | 说明 |
|---|---|---|
| token | TEXT PK | 随机高熵 token（≥32 字节熵） |
| user_id | INTEGER NOT NULL FK→users | |
| expires_at | TEXT NOT NULL | RFC3339 UTC，默认签发后 7 天 |
| created_at | TEXT NOT NULL | |

删除用户时必须级联删除其 sessions。

### snacks
| 列 | 类型 | 说明 |
|---|---|---|
| id | INTEGER PK AUTOINCREMENT | |
| name | TEXT NOT NULL | 零食名称 |
| quantity | INTEGER NOT NULL DEFAULT 1 | 数量，≥0 |
| location | TEXT NOT NULL | 存放位置，自由文本，如 `客厅/零食柜/第二层` |
| purchase_date | TEXT | 购买日期 `YYYY-MM-DD`，可空 |
| expiry_date | TEXT | 过期日期 `YYYY-MM-DD`，可空 |
| notes | TEXT | 备注，可空 |
| created_at | TEXT NOT NULL | RFC3339 UTC |
| updated_at | TEXT NOT NULL | RFC3339 UTC |

## 2. 认证方式

- 登录成功返回不透明随机 token（存 sessions 表）。前端存 localStorage，之后所有请求带 `Authorization: Bearer <token>`。
- 所有 `/api/*`（除 `POST /api/auth/login`）未带有效 token → `401`。
- 管理员专属接口对非管理员 → `403`。
- 统一错误体：`{"error": "人类可读信息"}`。

## 3. API 契约（前后端唯一事实来源，双方严格按此实现）

### 认证
| 方法/路径 | 请求体 | 成功响应 | 说明 |
|---|---|---|---|
| `POST /api/auth/login` | `{"username","password"}` | `{"token": string, "user": {"id","username","is_admin": bool}}` | 失败 401 |
| `POST /api/auth/logout` | — | `204` | 使当前 token 失效 |
| `GET /api/auth/me` | — | 同 login 的 `user` 对象 | |
| `POST /api/auth/change-password` | `{"old_password","new_password"}` | `204` | 所有登录用户可改自己密码；new_password ≥6 字符，否则 400 |

### 零食
| 方法/路径 | 说明 |
|---|---|
| `GET /api/snacks` | 查询参数：`q`（name/notes 模糊）、`location`（精确匹配）、`status`=`all\|ok\|expiring\|expired`（默认 `all`）。响应：Snack 数组（**裸数组，非包装对象**） |
| `POST /api/snacks` | 请求体见 SnackInput；201 返回 Snack |
| `PUT /api/snacks/{id}` | 请求体见 SnackInput；200 返回 Snack；不存在 404 |
| `DELETE /api/snacks/{id}` | `204`；不存在 404 |
| `GET /api/locations` | 去重后的存放位置字符串数组，按字典序 |

```jsonc
// SnackInput（POST/PUT 通用；日期为 YYYY-MM-DD 字符串，可为 null）
{ "name": "string(必填,非空)", "quantity": 1, "location": "string(必填,非空)",
  "purchase_date": "2026-01-05", "expiry_date": null, "notes": null }
// Snack = SnackInput + { "id": 1, "created_at": "...", "updated_at": "..." }
```
校验失败（缺字段、quantity<0、日期格式非 `YYYY-MM-DD`）→ 400 `{"error":...}`。

### 过期提醒
`GET /api/reminders` → 数组，元素为 Snack 字段再加：
```jsonc
{ ..., "days_left": -3,           // 今天到 expiry_date 的天数；无 expiry_date 的不出现
      "status": "expired" }       // expired（已过期）| critical（≤3天）| warning（≤7天）
```
仅返回 `expiry_date` 非空且 `days_left ≤ 7` 的零食，按 expiry_date 升序。`days_left < 0` → `expired`；`0..3` → `critical`；`4..7` → `warning`。**日期比较必须基于服务器本地日期（YYYY-MM-DD 字符串比较即可）**，因为这是家庭本地应用。

### 账户管理（仅 is_admin=1 可访问，非管理员 403）
| 方法/路径 | 请求体 | 响应 |
|---|---|---|
| `GET /api/users` | — | `[{"id","username","is_admin": bool,"created_at"}]` 数组 |
| `POST /api/users` | `{"username","password"}` | 201 返回同上单个对象；创建的账户 `is_admin=false`（**不提供角色管理功能**）；用户名重复 409 |
| `DELETE /api/users/{id}` | — | `204`；删除自己 → 400 `{"error":"不能删除当前登录账户"}`；不存在 404 |

## 4. 前端功能需求（中文界面）

页面（standalone components + Router + guards）：
1. **/login 登录页**：用户名/密码，错误提示。
2. **/ 零食库存**（需登录）：表格列 = 名称、数量、存放位置、购买日期、过期日期、状态徽章、备注、操作(编辑/删除)。状态徽章：已过期=红、≤7天=橙、正常=绿、无过期日期=灰。顶部：搜索框、存放位置下拉筛选（来自 `/api/locations`）、状态筛选、"+ 添加零食"按钮（弹窗或表单页均可）。
3. **/reminders 过期提醒**（需登录）：按 status 分组/标色展示，显示剩余天数。
4. **/users 账户管理**（需 admin guard，菜单项对非 admin 隐藏）：用户列表、添加账户、删除账户（含"不能删除自己"的错误展示）。
5. **/change-password 修改密码**（需登录）。
6. 顶部导航栏：显示当前用户名、退出登录；**库存页与导航栏需展示提醒角标（reminders 数量 > 0 时显示红色计数）**，数据来自 `/api/reminders`。

行为要求：
- HttpClient 统一携带 Bearer token；收到 401 自动跳转 /login 并清理本地 token。
- `authGuard` 保护所有业务路由；`adminGuard` 保护 /users。
- API 基址使用相对路径 `/api`（同源，无需代理配置）。
- 构建产物可被任意静态服务器 + SPA fallback 托管（不要求 base href 特殊配置，使用默认 `/`）。

## 5. 各 lane 验证要求（必须在报告中附命令与输出摘要）

- 后端：`cargo build`、`cargo test`（至少覆盖：登录、未授权 401、非管理员 403、snacks CRUD、reminders 分级计算、admin 用户增删及禁止删除自己）。可用 `cargo test` 内嵌 axum tower ServiceExt 测试，避免起真实端口。
- 前端：`ng build`（或 `npm run build`）成功，无 TypeScript 错误。
- 集成：`./build.sh` 一键成功；启动后端后用 curl 冒烟：登录→CRUD→reminders→admin 用户管理→`GET /` 返回 index.html→未知路径 SPA fallback。
