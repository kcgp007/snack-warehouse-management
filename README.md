# 零食仓库管理平台

管理家庭/仓库零食库存的单体 Web 应用：记录每件零食的**存放位置、生产日期、保质期、购买日期、过期日期**，在临期时给出**过期提醒**，并通过登录与管理员账户管理保护数据。

## 功能特性

- **零食库存管理**：名称、数量、存放位置、购买日期、生产日期、保质期（支持天/月/年单位）、过期日期、备注；**生产日期 / 保质期 / 过期日期填任意两项自动计算第三项**（三项矛盾会被拒绝）；支持按名称/备注搜索、按存放位置筛选；位置输入带**已有位置自动补全**（`GET /api/locations?q=`，可点击快选或自由输入）。
- **双重状态维度**：
  - 过期状态（基于过期日期）：已过期 / ≤7 天 / 正常 / 无过期日期；
  - 生命周期状态：`in_stock`（未打开）/ `opened`（已打开）/ `finished`（已吃完），并记录打开日期、吃完日期（纯 `YYYY-MM-DD`）；**状态无需手动选择，由打开/吃完日期自动推断**（仅填打开日期 → 已打开；填写吃完日期 → 已吃完；两者都为空 → 未打开），并校验“打开日期不能晚于吃完日期”，支持一键“标记打开 / 标记吃完”。
- **过期提醒**：`/reminders` 页面按分级（`expired` 已过期、`critical` ≤3 天、`warning` ≤7 天）展示即将过期的零食与剩余天数；已吃完的零食不再出现在提醒中；导航栏与库存页显示提醒角标。
- **登录认证**：登录后签发高熵 token（前端存 localStorage，所有请求携带 `Authorization: Bearer`），401 自动跳转登录页；支持退出登录、修改密码；启动时清理已过期会话。
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
├── build.sh         # 一键编译脚本（产物输出到 dist/）
├── backend/         # Rust 后端
│   ├── src/         # main.rs（入口/TOML 配置读取）、lib.rs（路由装配）、auth/snacks/reminders/users/db/static_files...
│   ├── tests/api.rs # 集成测试（tower::ServiceExt::oneshot，无需真实端口）
│   └── static/      # 前端构建产物（由 build.sh 生成，不入 git）
└── frontend/        # Angular 前端
    └── src/app/pages/{login,snacks,reminders,users,change-password}

dist/                # 编译产物目录（由 build.sh 生成，不入 git）
│   ├── snack-warehouse-backend   # 后端二进制
│   ├── static/                   # 前端静态资源
│   ├── run.sh                    # 一键运行脚本（HTTPS）
│   ├── install.sh                # 一键安装为 systemd 系统服务（开机自启）
│   └── uninstall.sh              # 一键卸载系统服务（交互式询问是否删除数据/用户）
```

## 快速开始

前置要求：Rust 工具链（cargo）、Node.js + npm。

```bash
# 1) 一键编译（npm install + ng build → 产物输出到 dist/，含后端二进制、static/、三部署脚本，可重复执行）
./build.sh

# 2) 进入产物目录运行（默认监听 0.0.0.0:3000，Ctrl-C 停止）
cd dist
./run.sh

# 如需部署到任意路径，直接把整个 dist/ 文件夹拷贝过去，再执行 ./run.sh 即可
# 路径由脚本自动推导，不依赖环境变量设置
```

浏览器访问 <https://127.0.0.1:3000> 即可使用（首次会提示自签名证书不受信任，选择继续访问即可，见下文「HTTPS 与证书」）。`run.sh` 同时打印局域网地址（如 `https://192.168.x.x:3000`），手机/其他电脑可直接打开。

手动方式（可选，仅在不使用 `run.sh` 时）：

```bash
./backend/target/debug/snack-warehouse-backend
```

### 配置文件（TOML）

服务启动时读取**与二进制同目录**的 `snack-warehouse.toml`（`dist/` 布局即 `dist/snack-warehouse.toml`；systemd 安装后为安装目录下的同名文件）。文件缺失或解析失败时回退默认值，**不读取任何环境变量**：

```toml
host = "0.0.0.0"
port = 3000
# db_path = "data/app.db"      # 可选，相对路径基于工作目录
# static_dir = "static"        # 可选，相对路径基于工作目录
admin_username = "admin"
admin_password = "admin123"
```

| 键 | 默认值 | 说明 |
|---|---|---|
| `host` | `0.0.0.0` | 监听地址，默认所有网卡（局域网可直接访问）；仅本机可设 `127.0.0.1` |
| `port` | `3000` | 监听端口（HTTP 与 HTTPS 共用） |
| `db_path` | `data/app.db` | SQLite 数据库路径，相对工作目录 |
| `static_dir` | `static` | 前端静态资源目录，相对工作目录 |
| `admin_username` | `admin` | 首次建库时创建的管理员用户名 |
| `admin_password` | `admin123` | 首次建库时创建的管理员密码（登录后请立即修改） |

`run.sh` 不导出任何环境变量，路径由脚本按 `dist/` 布局自动推导；systemd 安装时生成的配置文件权限为 `640 root:snack-warehouse`（管理员可编辑、服务可读但不可改），修改后执行 `sudo systemctl restart snack-warehouse` 生效。

### HTTPS 与证书

服务只以 HTTPS 提供内容，并且**同一端口会自动识别明文 HTTP**：用 `http://` 访问时返回 `308 Permanent Redirect` 跳转到对应的 `https://` 地址（路径与查询参数保留），HTTPS 响应同时带 `Strict-Transport-Security`，后续浏览器会自动使用 HTTPS。

首次启动时若 `data/certs/server.crt` 与 `data/certs/server.key` 都不存在，会自动生成一对自签名证书（私钥权限 `0600`），路径相对当前运行目录，即：

```text
<运行目录>/data/certs/server.crt
<运行目录>/data/certs/server.key
```

- 浏览器首次访问会提示“证书不受信任/连接不是私密连接”，这是自签名证书的正常现象，选择继续访问即可；手机/其他电脑首次访问同样需要手动信任。
- 生成的证书包含 `localhost`、`127.0.0.1`、`::1` 以及生成时探测到的本机局域网 IPv4；**如果服务器 IP 变化**，删除这两个文件后重启服务即可重新生成。
- 如需使用受信任证书（如 Let's Encrypt 签发），把证书与私钥分别覆盖为上述两个文件名（PEM 格式），然后重启服务；若只存在其中一个文件，服务会报错而不会自动覆盖。
- `curl` 访问自签名证书需加 `-k`：`curl -k https://127.0.0.1:3000/`。

### 远程无法访问？

1. **确认监听地址**：默认 `HOST=0.0.0.0`（所有网卡）。启动日志应显示 `[server] listening on https://0.0.0.0:3000`；若设了 `HOST=127.0.0.1` 则仅本机可访问。
2. **防火墙放行**：放行 TCP 3000（或你自定义的 `port`）。例如 `sudo ufw allow 3000/tcp`（Ubuntu）。
3. **用局域网 IP 访问**：在服务器上执行 `hostname -I` 取 IP，其他设备浏览器打开 `https://<该IP>:3000`；两端需在同一局域网（或已配置路由/端口转发）。
4. **容器部署**：需同时映射端口（如 `-p 3000:3000`），`HOST` 保持默认 `0.0.0.0` 即可。

### 默认账号与安全提示

- 首次启动时若库中不存在同名用户，会自动创建默认管理员：**用户名 `admin` / 密码 `admin123`**（启动日志会打印一行提示）。
- 初始化是幂等的：**已存在的同名用户绝不会被覆盖或重置密码**；可通过 TOML 的 `admin_username` / `admin_password` 覆盖初始账号。
- ⚠️ **请在首次登录后立即通过 `/change-password` 修改默认密码**，不要在生产环境保留 `admin123`。

## 系统级安装（systemd）

Linux + systemd 环境可把应用安装为系统级守护进程（开机自启、崩溃自动重启），需要 root：

```bash
cd dist                              # 进入编译产物目录
sudo ./install.sh                    # 安装/升级：安装位置 = 本脚本所在目录
sudo ./install.sh --no-build         # 使用现有编译产物
sudo ./install.sh --profile release  # 使用 release 后端（仍会先构建前端）
```

安装位置与运行用户是固定的，不需要也不允许通过参数或环境变量指定：

- **安装目录**：`/opt/snack-warehouse`。`install.sh` 可放在任意位置（例如家目录下的 `dist/`），运行时会把二进制、`static/`、两个脚本与 README 部署到 `/opt/snack-warehouse`；
- **运行用户**：`snack-warehouse`，不存在时自动创建为无登录 shell 的系统用户；程序归 `root`，服务仅写 `data/`；
- **不做数据迁移**：`/opt/snack-warehouse/data` 与配置已存在则原样保留，否则全新初始化；
- **路径兼容检查**：`noexec` 挂载、危险路径、运行用户无法穿越安装目录等情况会在启动服务前报错；必要时用 `setfacl` 仅授予穿越权限；
- 升级时递归纠正 `data/` 所有权、配置 `640`、程序与静态资源 `a+rX`；旧 unit 中的 `User=` 会被重写为 `snack-warehouse`。

其余行为：

- 以仓库所有者身份执行 `./build.sh`（避免在 `target/` 下留下 root 所有的文件）；
- 首次安装生成 `<安装目录>/snack-warehouse.toml`（TOML，`root:snack-warehouse` 权限 `640`），升级时保留已有配置并自动纠正权限；
- 写入 `/etc/systemd/system/snack-warehouse.service`：`User=snack-warehouse`，`Restart=on-failure`、`ProtectSystem=strict`、仅数据目录可写；安装在 `/home` 下时自动改用 `ProtectHome=read-only`；
- 设为开机自启并启动，通过 `curl -k` 做启动健康检查（自签名证书）；
- **不使用任何环境变量**：配置只来自运行目录下的 TOML。

常用运维命令：

```bash
sudo systemctl status snack-warehouse
sudo systemctl restart snack-warehouse
sudo journalctl -u snack-warehouse -f
sudoedit /path/to/install/snack-warehouse.toml   # 改端口/初始管理员等，保存后重启服务生效
```

卸载：

```bash
cd /opt/snack-warehouse          # 安装目录（unit 的 WorkingDirectory 中也可查到）
sudo ./uninstall.sh              # 交互式卸载；询问是否同时删除数据库/配置、运行用户
```

卸载脚本会从 unit 的 `WorkingDirectory=` / `User=` / `Group=` 读取**实际安装目录与运行用户**（unit 不存在时回退到脚本所在目录），不做预判。过程不接受“一键全删”参数，而是分步手动确认（默认均为 N，直接回车即保留）：

1. `是否同时删除数据库与配置？` —— 输入 `y` 才会删除 `data/`（含 `app.db` 与自签证书）和 `snack-warehouse.toml`；
2. `是否同时删除系统用户与用户组 <运行用户>？` —— 仅当该用户是系统账户（uid < 1000）才会询问，输入 `y` 才执行 `userdel`/`groupdel`；普通账户（如 `king`）**永不删除**，只提示保留。

无论选择如何，systemd 服务、unit 文件、程序二进制与静态资源都会被删除；未选择删除的部分会保留，重新安装可直接复用。回答 `y`/`yes`（不区分大小写）视为同意，其他任何输入（含直接回车、EOF）视为取消。

> 整个 `dist/` 文件夹可拷贝到任意服务器路径，进入后执行 `./run.sh` 即可直接运行；如需 systemd 托管，在该目录下执行 `sudo ./install.sh`。安装后请立即登录修改默认密码 `admin123`；对外提供服务时记得防火墙放行配置中的 `port`（默认 3000/tcp）。

## API 概览

所有 `/api/*` 接口（除登录外）都需要请求头 `Authorization: Bearer <token>`，未带有效 token → `401`；管理员接口对非管理员 → `403`；错误体统一为 `{"error": "人类可读信息"}`。

| 方法/路径 | 说明 |
|---|---|
| `POST /api/auth/login` | `{"username","password"}` → `{"token","user"}`；失败 `401`（唯一免 token 接口） |
| `POST /api/auth/logout` | `204`，使当前 token 失效 |
| `GET /api/auth/me` | 当前用户对象 |
| `POST /api/auth/change-password` | `{"old_password","new_password"}` → `204`；新密码 <6 字符 → `400` |
| `GET /api/snacks` | 查询参数 `q`/`location`/`expiry_status`(all\|ok\|expiring\|expired)/`status`(all\|in_stock\|opened\|finished)，返回**裸数组**，默认按创建时间倒序（`created_at DESC, id DESC`） |
| `POST /api/snacks` | 创建零食，`201`；校验失败 `400`；`production_date`/`opened_at`/`finished_at` 只接受 `YYYY-MM-DD`（传 `YYYY-MM-DDTHH:MM` → `400`）；`production_date`+`shelf_life_days`+`expiry_date` 填两项自动算第三项，三项矛盾 → `400`；`shelf_life_unit` 可选 `day/month/year`（1 月 = 30 天、1 年 = 365 天，仅用于换算与回显）；`status` 由日期推断，显式传入且与日期不一致 → `400`；`opened_at > finished_at` → `400` |
| `PUT /api/snacks/{id}` | 更新，`200`；不存在 `404`；日期校验同创建 |
| `DELETE /api/snacks/{id}` | `204`；不存在 `404` |
| `GET /api/locations` | 存放位置去重数组（字典序）；可选 `q` 做子串包含过滤，缺省/空串返回全部 |
| `GET /api/reminders` | Snack 字段 + `days_left` + `level`（`expired`\|`critical`\|`warning`），排除 `finished`，按过期日期升序 |
| `GET /api/users` `POST /api/users` | 仅管理员（非管理员 `403`）；列表按用户名升序；创建 `201`，用户名重复 `409` |
| `DELETE /api/users/{id}` | `204`；删除自己 `400 {"error":"不能删除当前登录账户"}`；不存在 `404` |
| 非 `/api` 路径 | 托管静态文件；磁盘无对应文件时回退 `index.html`（SPA fallback） |

启动时会幂等迁移旧库：自动补 `production_date`/`shelf_life_days`/`shelf_life_unit` 列；生命周期时间 `YYYY-MM-DDTHH:MM` 截取前 10 位为纯日期，非法值置 `NULL`；清理过期会话。

完整契约见 [`CONTRACT.md`](CONTRACT.md)。

## 测试

```bash
# 后端：单元测试 + 集成测试（登录、401/403、snacks CRUD 与 v2 日期校验、生产日期/保质期联动、启动迁移、
# 位置过滤/通配符转义、405 JSON、reminders 分级、用户管理、admin 幂等初始化、静态/SPA 托管等）
cd backend && cargo test

# 前端：单元测试（vitest）
cd frontend && npm test
```

另可执行 `./build.sh` 验证一键编译，再按“快速开始”启动后端用 `curl` 做端到端冒烟（登录 → CRUD → reminders → 用户管理 → `GET /` → SPA fallback）。

列表排序约定：库存按 `created_at DESC, id DESC`（最新创建在前）；账户管理按用户名升序；过期提醒按过期日期升序。
