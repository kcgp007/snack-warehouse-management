# 零食仓库管理平台

管理家庭/仓库零食库存的单体 Web 应用：记录每件零食的**存放位置、购买日期、过期日期**，在临期时给出**过期提醒**，并通过登录与管理员账户管理保护数据。

## 功能特性

- **零食库存管理**：名称、数量、存放位置、购买日期、过期日期、备注；支持按名称/备注搜索、按存放位置筛选。
- **双重状态维度**：
  - 过期状态（基于过期日期）：已过期 / ≤7 天 / 正常 / 无过期日期；
  - 生命周期状态：`in_stock`（未打开）/ `opened`（已打开）/ `finished`（已吃完），并记录打开时间、吃完时间（状态选“已吃完”而吃完时间留空时由服务端自动填充），支持一键“标记打开 / 标记吃完”。
- **过期提醒**：`/reminders` 页面按分级（`expired` 已过期、`critical` ≤3 天、`warning` ≤7 天）展示即将过期的零食与剩余天数；已吃完的零食不再出现在提醒中；导航栏与库存页显示提醒角标。
- **登录认证**：登录后签发高熵 token（前端存 localStorage，所有请求携带 `Authorization: Bearer`），401 自动跳转登录页；支持退出登录、修改密码。
- **admin 账户管理**：管理员可查看/创建/删除普通账户（不能删除自己）；非管理员访问账户管理接口返回 403；首次启动自动初始化 admin 账户。
- **单进程部署**：无 Nginx / API 网关 / 独立认证服务等中间件，Angular 构建产物由 Rust 后端直接托管（含 SPA fallback）。

## 技术栈

| 层 | 技术 |
|---|---|
| 后端 | Rust + axum 0.8 + tokio，rusqlite（bundled SQLite），argon2 密码哈希 |
| 数据库 | SQLite（默认 `data/app.db`，首次启动自动建表） |
| 前端 | Angular（standalone components + Router + guards，无 NgRx），中文界面 |
| 部署 | 单体：同一 Rust 进程同时提供 `/api/*` REST 接口与前端静态资源 |

## 目录结构

```
snack-warehouse-management/
├── CONTRACT.md      # 前后端集成契约（唯一事实来源）
├── README.md        # 本文件
├── build.sh         # 一键编译脚本
├── run.sh           # 一键运行脚本（必要时先自动编译）
├── backend/         # Rust 后端
│   ├── src/         # main.rs（入口/环境变量）、lib.rs（路由装配）、auth/snacks/reminders/users/db/static_files...
│   ├── tests/api.rs # 集成测试（tower::ServiceExt::oneshot，无需真实端口）
│   └── static/      # 前端构建产物（由 build.sh 生成，不入 git）
└── frontend/        # Angular 前端
    └── src/app/pages/{login,snacks,reminders,users,change-password}
```

## 快速开始

前置要求：Rust 工具链（cargo）、Node.js + npm。

```bash
# 1) 一键编译（npm install + ng build → 产物拷贝到 backend/static/ → cargo build，可重复执行）
./build.sh

# 2) 一键运行（必要时会先自动 ./build.sh；默认监听 0.0.0.0:3000，Ctrl-C 停止）
./run.sh
```

浏览器访问 <http://127.0.0.1:3000> 即可使用。`run.sh` 同时打印局域网地址（如 `http://192.168.x.x:3000`），手机/其他电脑可直接打开。

手动方式（可选，仅在不使用 `run.sh` 时）：

```bash
./backend/target/debug/snack-warehouse-backend
```

### 环境变量

| 变量 | 默认值 | 说明 |
|---|---|---|
| `SNACK_DB_PATH` | `data/app.db` | SQLite 数据库文件路径（`data/` 已被 .gitignore 忽略） |
| `STATIC_DIR` | `backend/static` | 静态资源目录（编译期按仓库路径解析，与 cwd 无关） |
| `HOST` | `0.0.0.0` | 监听地址，默认监听所有网卡（局域网可直接远程访问）；仅本机访问可设 `127.0.0.1` |
| `PORT` | `3000` | 监听端口 |
| `ADMIN_USERNAME` | `admin` | 首次启动自动初始化的管理员用户名 |
| `ADMIN_PASSWORD` | `admin123` | 首次启动自动初始化的管理员密码 |

经 `./run.sh` 启动时，脚本会为未设置的变量填入上表默认值，并把相对路径（如 `data/app.db`）按仓库根目录解析。

### 远程无法访问？

1. **确认监听地址**：默认 `HOST=0.0.0.0`（所有网卡）。启动日志应显示 `[server] listening on http://0.0.0.0:3000`；若设了 `HOST=127.0.0.1` 则仅本机可访问。
2. **防火墙放行**：放行 TCP 3000（或你自定义的 `PORT`）。例如 `sudo ufw allow 3000/tcp`（Ubuntu）。
3. **用局域网 IP 访问**：在服务器上执行 `hostname -I` 取 IP，其他设备浏览器打开 `http://<该IP>:3000`；两端需在同一局域网（或已配置路由/端口转发）。
4. **容器部署**：需同时映射端口（如 `-p 3000:3000`），`HOST` 保持默认 `0.0.0.0` 即可。

### 默认账号与安全提示

- 首次启动时若库中不存在同名用户，会自动创建默认管理员：**用户名 `admin` / 密码 `admin123`**（启动日志会打印一行提示）。
- 初始化是幂等的：**已存在的同名用户绝不会被覆盖或重置密码**；可通过 `ADMIN_USERNAME` / `ADMIN_PASSWORD` 覆盖初始账号。
- ⚠️ **请在首次登录后立即通过 `/change-password` 修改默认密码**，不要在生产环境保留 `admin123`。

## API 概览

所有 `/api/*` 接口（除登录外）都需要请求头 `Authorization: Bearer <token>`，未带有效 token → `401`；管理员接口对非管理员 → `403`；错误体统一为 `{"error": "人类可读信息"}`。

| 方法/路径 | 说明 |
|---|---|
| `POST /api/auth/login` | `{"username","password"}` → `{"token","user"}`；失败 `401`（唯一免 token 接口） |
| `POST /api/auth/logout` | `204`，使当前 token 失效 |
| `GET /api/auth/me` | 当前用户对象 |
| `POST /api/auth/change-password` | `{"old_password","new_password"}` → `204`；新密码 <6 字符 → `400` |
| `GET /api/snacks` | 查询参数 `q`/`location`/`expiry_status`(all\|ok\|expiring\|expired)/`status`(all\|in_stock\|opened\|finished)，返回**裸数组** |
| `POST /api/snacks` | 创建零食，`201`；校验失败 `400`；`status="finished"` 且未带 `finished_at` 时服务端自动填充 |
| `PUT /api/snacks/{id}` | 更新，`200`；不存在 `404` |
| `DELETE /api/snacks/{id}` | `204`；不存在 `404` |
| `GET /api/locations` | 存放位置去重数组（字典序） |
| `GET /api/reminders` | Snack 字段 + `days_left` + `level`（`expired`\|`critical`\|`warning`），排除 `finished`，按过期日期升序 |
| `GET /api/users` `POST /api/users` | 仅管理员（非管理员 `403`）；创建 `201`，用户名重复 `409` |
| `DELETE /api/users/{id}` | `204`；删除自己 `400 {"error":"不能删除当前登录账户"}`；不存在 `404` |
| 非 `/api` 路径 | 托管静态文件；磁盘无对应文件时回退 `index.html`（SPA fallback） |

完整契约见 [`CONTRACT.md`](CONTRACT.md)。

## 测试

```bash
# 后端：单元测试 + 集成测试（登录、401/403、snacks CRUD 与校验、reminders 分级、
# 用户管理与“不能删除自己”、admin 幂等初始化、静态/SPA 托管等）
cd backend && cargo test

# 前端：单元测试（vitest）
cd frontend && npm test
```

另可执行 `./build.sh` 验证一键编译，再按“快速开始”启动后端用 `curl` 做端到端冒烟（登录 → CRUD → reminders → 用户管理 → `GET /` → SPA fallback）。
