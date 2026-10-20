# 零食仓库管理平台 — 集成契约（父级决策，子agent必须遵守）

> **契约版本：v2**（2026-10-08 变更，取代 v1 对应条目）：
> 1. `opened_at` / `finished_at` 由 `YYYY-MM-DDTHH:MM` 改为**纯日期 `YYYY-MM-DD`**（含存量数据迁移、校验、自动填充、前端 `date` 输入与展示）；
> 2. `GET /api/locations` 新增查询参数 **`q`**，用于存放位置输入框“按已输入字符回显已有位置、可快选”的自动补全；
> 3. 前端整体美化（视觉/交互一致性为验收项）。
> v2 未提及的条目沿用 v1。

## 0. 总体架构（不可更改的约束）

- **单体部署，无独立中间件**：不使用 Nginx / API 网关 / 独立认证服务。Rust 后端同时承担静态资源托管，Angular 编译产物直接由 Rust 进程提供服务。
- **技术栈**：后端 Rust（推荐 axum + tokio + rusqlite(bundled)），数据库 SQLite，前端 Angular（standalone components，无 NgRx）。
- **仓库布局（目录所有权，见各子任务）**：
  ```
  snack-warehouse-management/
  ├── CONTRACT.md          # 本文件（父级所有，子agent只读）
  ├── README.md            # 集成阶段所有者编写
  ├── build.sh             # 一键编译脚本（集成阶段所有者编写）
  ├── run.sh               # 一键运行脚本（集成阶段所有者编写）
  ├── .gitignore           # 父级所有，子agent不得修改
  ├── backend/             # 后端 lane 独占
  └── frontend/            # 前端 lane 独占
  ```
- **静态资源托管规则**：
  - 后端从 `backend/static/` 目录提供静态文件（目录可由环境变量 `STATIC_DIR` 覆盖，默认 `backend/static`）。
  - `/api/*` 路径永远优先于静态路由；非 `/api` 且磁盘上无对应文件的路径一律回退到 `index.html`（SPA fallback）。
  - **一键编译 `build.sh`（repo 根，bash，`set -euo pipefail`，可重复执行）**：
  1) `cd frontend && npm install && npm run build`；
  2) 把 Angular 构建产物目录（含 index.html 的 dist 子目录）内容拷贝到 `backend/static/`（保证 `backend/static/index.html` 存在）；
  3) `cd backend && cargo build`（debug 即可）。
  结束时打印“编译成功”及下一步提示（运行 `./run.sh`）。
- **一键运行 `run.sh`（repo 根，bash，`set -euo pipefail`）**：
  1) 若 `backend/static/index.html` 或后端二进制不存在，先自动调用 `./build.sh`；
  2) 以环境变量默认值启动后端（支持 `PORT` 默认 3000、`STATIC_DIR`、`SNACK_DB_PATH` 默认 `data/app.db`、`ADMIN_USERNAME`/`ADMIN_PASSWORD`）；
  3) 前台运行并转发信号（trap SIGINT/SIGTERM，Ctrl-C 能干净退出子进程）；启动成功后打印访问地址（如 `https://127.0.0.1:3000`；同一端口自动识别明文 HTTP 并 308 跳转到 HTTPS；首次运行自动生成自签名证书到 `data/certs/`）。
  两脚本均需 `chmod +x`，并在 README 的“快速开始”中作为主要方式说明。
- `backend/static/` 及其内容不提交到 git（.gitignore 已忽略），由 build.sh 生成。

## 1. 数据模型（SQLite）

默认数据库文件：`data/app.db`（可用环境变量 `SNACK_DB_PATH` 覆盖；`data/` 已被 .gitignore 忽略）。首次启动自动建表。

**admin 用户自动初始化（后端启动时执行，幂等）**：
- 启动时若 `users` 表为空（或不存在名为 `admin` 的用户），自动创建默认管理员：用户名 `admin`、密码 `admin123`，`is_admin=1`。
- 支持环境变量覆盖初始账号：`ADMIN_USERNAME`（默认 `admin`）、`ADMIN_PASSWORD`（默认 `admin123`）。
- 幂等保证：每次启动都检查，**已存在同名用户则绝不覆盖/重置其密码**（即使用户后续改过密码）；初始化发生时向日志打印一行提示（含用户名，**不打印密码以外的敏感信息，密码仅在使用了非默认值时可提示“已通过 ADMIN_PASSWORD 设置”**）。
- README 必须说明：自动初始化行为、环境变量、默认密码，以及登录后立即修改密码。

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
| production_date | TEXT | 生产日期 `YYYY-MM-DD`，可空（v3 新增） |
| shelf_life_days | INTEGER | 保质期（天），`0..=36500`，可空（v3 新增） |
| expiry_date | TEXT | 过期日期 `YYYY-MM-DD`，可空 |
| opened_at | TEXT | 打开日期 `YYYY-MM-DD`（服务器本地日期，**纯日期，无时间部分**），**非必填**，可空 |
| finished_at | TEXT | 吃完日期 `YYYY-MM-DD`（纯日期），可空 |
| status | TEXT NOT NULL DEFAULT 'in_stock' | 生命周期状态：`in_stock`（未打开）/ `opened`（已打开）/ `finished`（已吃完） |
| notes | TEXT | 备注，可空 |
| created_at | TEXT NOT NULL | RFC3339 UTC |
| updated_at | TEXT NOT NULL | RFC3339 UTC |

**存量数据迁移（v2 必做，幂等）**：v1 时代写入的 `opened_at`/`finished_at` 若为 `YYYY-MM-DDTHH:MM`（或更长格式），后端启动迁移时统一**截取前 10 位**规范化为 `YYYY-MM-DD`（非法且无法截取的值置 NULL）；迁移必须幂等，重复启动无副作用。启动后所有读写路径只接受/输出 `YYYY-MM-DD`。

**存量数据迁移（v3，幂等）**：旧库启动时通过 `PRAGMA table_info` 检测并 `ALTER TABLE` 补齐 `production_date` / `shelf_life_days` 列，已有数据两列为 NULL。

**生产日期 / 保质期 / 过期日期联动**：三项均可空；**填任意两项时服务端自动计算第三项**（生产日期 + 保质期 → 过期日期；生产日期 + 过期日期 → 保质期天数；保质期 + 过期日期 → 生产日期）。三项都填且互不自洽 → 400 `{"error":"生产日期、保质期与过期日期不一致"}`；过期日期早于生产日期 → 400 `{"error":"过期日期不能早于生产日期"}`；保质期不在 `0..=36500` → 400。前端以用户最后修改的字段为准覆盖另一项，保证界面上三项始终自洽。

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
| `GET /api/snacks` | 查询参数：`q`（name/notes 模糊）、`location`（精确匹配）、`expiry_status`=`all\|ok\|expiring\|expired`（默认 `all`，**过期维度**）、`status`=`all\|in_stock\|opened\|finished`（默认 `all`，**生命周期维度**）。响应：Snack 数组（**裸数组，非包装对象**） |
| `POST /api/snacks` | 请求体见 SnackInput；201 返回 Snack |
| `PUT /api/snacks/{id}` | 请求体见 SnackInput；200 返回 Snack；不存在 404 |
| `DELETE /api/snacks/{id}` | `204`；不存在 404 |
| `GET /api/locations` | 去重后的存放位置字符串数组，按字典序。查询参数 **`q`（可选，v2 新增）**：按**子串包含**过滤（如 `q=客厅` 只返回含“客厅”的位置），`q` 缺失或为空串返回全部；专用于存放位置输入框的自动补全回显 |

```jsonc
// SnackInput（POST/PUT 通用；所有日期字段均为 YYYY-MM-DD，可为 null）
{ "name": "string(必填,非空)", "quantity": 1, "location": "string(必填,非空)",
  "purchase_date": "2026-01-05", "production_date": null,
  "shelf_life_days": null, "expiry_date": null, "notes": null,
  // 以下为新增字段，均可省略；省略时 status 默认 'in_stock'，两个日期为 null
  "status": "in_stock",          // 可选值 in_stock|opened|finished，非法值 400
  "opened_at": null,              // 打开日期 YYYY-MM-DD，非必填
  "finished_at": null }           // 吃完日期 YYYY-MM-DD
// Snack = SnackInput + { "id": 1, "created_at": "...", "updated_at": "..." }
```
校验失败（缺字段、quantity<0、`purchase_date`/`production_date`/`expiry_date`/`opened_at`/`finished_at` 任一格式非 `YYYY-MM-DD`（v2 起**不再接受** `YYYY-MM-DDTHH:MM`，传入即 400）、`shelf_life_days` 非整数或越界、三项日期互不自洽、status 非法）→ 400 `{"error":...}`。
服务端规则：`status="finished"` 且请求未带 `finished_at` 时，服务端自动填充**当前本地日期 `YYYY-MM-DD`**；`finished_at` 仅在 `status="finished"` 时有意义（其他状态下携带可保存但不强制清除，前后端 UI 不应展示矛盾数据）。

### 过期提醒
`GET /api/reminders` → 数组，元素为 Snack 字段（**含新字段 status/opened_at/finished_at**）再加：
```jsonc
{ ..., "days_left": -3,            // 今天到 expiry_date 的天数
      "level": "expired" }         // expired（已过期）| critical（≤3天）| warning（≤7天）
```
注意：提醒分级字段名为 **`level`**（Snack 自身的 `status` 是生命周期状态，两者不可混用）。
**排除规则：`status == "finished"`（已吃完）的零食不出现在提醒中。**
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
2. **/ 零食库存**（需登录）：表格列 = 名称、数量、存放位置、购买日期、**生产日期**、**保质期（天）**、过期日期、**过期状态徽章**、**生命周期状态徽章**、**打开日期**、**吃完日期**、备注、操作(编辑/删除)。
   - 过期状态徽章（基于 expiry_date）：已过期=红、≤7天=橙、正常=绿、无过期日期=灰。
   - 生命周期状态徽章（status）：`in_stock`=蓝(未打开)、`opened`=紫(已打开)、`finished`=灰(已吃完)。
   - 顶部筛选：搜索框、存放位置下拉（来自 `/api/locations`）、过期状态筛选（请求参数 `expiry_status`）、生命周期状态筛选（请求参数 `status`)、"+ 添加零食"按钮（弹窗或表单页均可）。
   - 编辑/添加表单字段含：生产日期、保质期（天）、过期日期（三者填任意两项自动计算第三项，并即时提示日期矛盾）、状态下拉、打开日期（**`type="date"` 输入，选填**）、吃完日期（`type="date"`，选填；状态选"已吃完"且吃完日期留空时由后端自动填充）。
   - **存放位置输入框自动补全（v2 必做）**：输入过程中按**已输入字符**调用 `GET /api/locations?q=<已输入文本>`，在输入框下方回显匹配的已有位置列表（防抖即可，非强制），**点击可快选填入**；同时必须仍允许输入全新位置自由保存（自动补全只是回显建议，不做强制约束）。筛选栏的存放位置下拉沿用 `GET /api/locations`（无 `q`）。
   - 建议在操作列提供快捷动作："标记打开"（status→opened，opened_at 取当前日期）与"标记吃完"（status→finished，finished_at 取当前日期），即一次 PUT 请求完成。
   - **整体美化（v2 验收项）**：统一配色/字号/间距/圆角/阴影，徽章与按钮风格一致，表格行悬停与斑马纹，空状态与加载态提示，表单校验错误即时可见，桌面与窄屏（移动）布局均不破版；中文界面文案统一。
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

- 后端：`cargo build`、`cargo test`（至少覆盖：登录、未授权 401、非管理员 403、snacks CRUD、reminders 分级计算、admin 用户增删及禁止删除自己、**admin 自动初始化：空库启动创建默认 admin、重复启动不重置已改密码的 admin、ADMIN_USERNAME/ADMIN_PASSWORD 覆盖生效**、**v3：生产日期/保质期/过期日期任意两项自动推导第三项、三项矛盾与过期早于生产 → 400、旧库自动补列且幂等**、**v2：opened_at/finished_at 纯日期校验（接受 `YYYY-MM-DD`，拒绝 `YYYY-MM-DDTHH:MM`）、status=finished 自动填充当前本地日期、存量 `YYYY-MM-DDTHH:MM` 迁移截断为 `YYYY-MM-DD` 且幂等、`GET /api/locations?q=` 子串过滤与无参返回全部**）。可用 `cargo test` 内嵌 axum tower ServiceExt 测试，避免起真实端口。
- 前端：`ng build`（或 `npm run build`）成功，无 TypeScript 错误；**`npm test`（vitest）全绿**。
- 集成：`./build.sh` 一键编译成功；`./run.sh` 一键运行后用 curl 冒烟：登录→CRUD→reminders→admin 用户管理→`GET /` 返回 index.html→未知路径 SPA fallback→Ctrl-C 干净退出（进程无残留）。
