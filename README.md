# 零食仓库管理平台

管理家庭/仓库零食库存的单体 Web 应用：记录每件零食的**存放位置、生产日期、保质期、购买日期、过期日期**，在临期时给出**过期提醒**，并通过登录与管理员账户管理保护数据。

## 功能特性

- **零食库存管理**：名称、数量、存放位置、购买日期、生产日期、保质期（天）、过期日期、备注；**生产日期 / 保质期 / 过期日期填任意两项自动计算第三项**（三项矛盾会被拒绝）；支持按名称/备注搜索、按存放位置筛选；位置输入带**已有位置自动补全**（`GET /api/locations?q=`，可点击快选或自由输入）。
- **双重状态维度**：
  - 过期状态（基于过期日期）：已过期 / ≤7 天 / 正常 / 无过期日期；
  - 生命周期状态：`in_stock`（未打开）/ `opened`（已打开）/ `finished`（已吃完），并记录打开日期、吃完日期（契约 v2：纯 `YYYY-MM-DD`；状态选“已吃完”而吃完日期留空时由服务端自动填充当前本地日期），支持一键“标记打开 / 标记吃完”。
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
│   ├── src/         # main.rs（入口/环境变量）、lib.rs（路由装配）、auth/snacks/reminders/users/db/static_files...
│   ├── tests/api.rs # 集成测试（tower::ServiceExt::oneshot，无需真实端口）
│   └── static/      # 前端构建产物（由 build.sh 生成，不入 git）
└── frontend/        # Angular 前端
    └── src/app/pages/{login,snacks,reminders,users,change-password}

dist/                # 编译产物目录（由 build.sh 生成，不入 git）
│   ├── snack-warehouse-backend   # 后端二进制
│   ├── static/                   # 前端静态资源
│   ├── run.sh                    # 一键运行脚本（HTTPS）
│   ├── install.sh                # 一键安装为 systemd 系统服务（开机自启）
│   └── uninstall.sh              # 一键卸载系统服务（可选 --purge 删除数据）
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

### 环境变量

| 变量 | 默认值 | 说明 |
|---|---|---|
| `PORT` | `3000` | 监听端口 |
| `HOST` | `0.0.0.0` | 监听地址，默认监听所有网卡（局域网可直接远程访问）；仅本机访问可设 `127.0.0.1` |
| `ADMIN_USERNAME` | `admin` | 首次启动自动初始化的管理员用户名 |
| `ADMIN_PASSWORD` | `admin123` | 首次启动自动初始化的管理员密码 |

核心路径（后端二进制、静态资源目录、数据库路径）由脚本自动推导（优先当前目录 `dist/`，回退仓库 `backend/` 布局），**不通过环境变量设置**；可通过 `PORT`/`HOST`/`ADMIN_USERNAME`/`ADMIN_PASSWORD` 覆盖非路径配置。

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
2. **防火墙放行**：放行 TCP 3000（或你自定义的 `PORT`）。例如 `sudo ufw allow 3000/tcp`（Ubuntu）。
3. **用局域网 IP 访问**：在服务器上执行 `hostname -I` 取 IP，其他设备浏览器打开 `https://<该IP>:3000`；两端需在同一局域网（或已配置路由/端口转发）。
4. **容器部署**：需同时映射端口（如 `-p 3000:3000`），`HOST` 保持默认 `0.0.0.0` 即可。

### 默认账号与安全提示

- 首次启动时若库中不存在同名用户，会自动创建默认管理员：**用户名 `admin` / 密码 `admin123`**（启动日志会打印一行提示）。
- 初始化是幂等的：**已存在的同名用户绝不会被覆盖或重置密码**；可通过 `ADMIN_USERNAME` / `ADMIN_PASSWORD` 覆盖初始账号。
- ⚠️ **请在首次登录后立即通过 `/change-password` 修改默认密码**，不要在生产环境保留 `admin123`。

## 系统级安装（systemd）

Linux + systemd 环境可把应用安装为系统级守护进程（开机自启、崩溃自动重启），需要 root：

```bash
cd dist                          # 进入编译产物目录
sudo ./install.sh                # 编译 + 安装/升级（就地安装，以 install.sh 所在目录为运行目录）
sudo ./install.sh --no-build     # 使用现有编译产物
sudo PROFILE=release ./install.sh # 使用 release 后端（仍会先构建前端）
```

安装脚本会：

- 以仓库所有者身份执行 `./build.sh`（避免在 `target/` 下留下 root 所有的文件）；
- 创建无登录 shell 的系统用户 `snack-warehouse`；
- **默认就地安装**：程序与静态资源放在 `install.sh` 所在目录（即 `dist/`），数据库放在其下的 `data/` 子目录；
- 路径由脚本自动推导，**不通过环境变量设置核心路径**（`INSTALL_DIR`/`DATA_DIR`/`SNACK_DB_PATH`/`STATIC_DIR` 等环境变量不再生效）；
- 首次安装生成 `/etc/snack-warehouse.env`（root 600，仅保留 PORT/HOST/ADMIN_USERNAME/ADMIN_PASSWORD），升级时保留已有配置；
- 写入 `/etc/systemd/system/snack-warehouse.service`（`Restart=on-failure`、`ProtectSystem=strict`、仅数据目录可写），设为开机自启并启动；
- 通过 `curl -k` 做启动健康检查（自签名证书）。

常用运维命令：

```bash
sudo systemctl status snack-warehouse
sudo systemctl restart snack-warehouse
sudo journalctl -u snack-warehouse -f
sudoedit /etc/snack-warehouse.env    # 改端口/初始管理员等，保存后重启服务生效
```

卸载：

```bash
cd dist                          # 进入安装时所在的目录（即 dist/）
sudo ./uninstall.sh            # 移除服务与程序，保留配置和数据库
sudo ./uninstall.sh --purge    # 连同配置、数据库、运行用户一并删除（会二次确认）
sudo ./uninstall.sh --purge --yes
```

> 整个 `dist/` 文件夹可拷贝到任意服务器路径，进入后执行 `./run.sh` 即可直接运行；如需 systemd 托管，在该目录下执行 `sudo ./install.sh`。安装后请立即登录修改默认密码 `admin123`；对外提供服务时记得防火墙放行 `PORT`（默认 3000/tcp）。

## API 概览

所有 `/api/*` 接口（除登录外）都需要请求头 `Authorization: Bearer <token>`，未带有效 token → `401`；管理员接口对非管理员 → `403`；错误体统一为 `{"error": "人类可读信息"}`。

| 方法/路径 | 说明 |
|---|---|
| `POST /api/auth/login` | `{"username","password"}` → `{"token","user"}`；失败 `401`（唯一免 token 接口） |
| `POST /api/auth/logout` | `204`，使当前 token 失效 |
| `GET /api/auth/me` | 当前用户对象 |
| `POST /api/auth/change-password` | `{"old_password","new_password"}` → `204`；新密码 <6 字符 → `400` |
| `GET /api/snacks` | 查询参数 `q`/`location`/`expiry_status`(all\|ok\|expiring\|expired)/`status`(all\|in_stock\|opened\|finished)，返回**裸数组** |
| `POST /api/snacks` | 创建零食，`201`；校验失败 `400`；`production_date`/`opened_at`/`finished_at` 只接受 `YYYY-MM-DD`（传 `YYYY-MM-DDTHH:MM` → `400`）；`production_date`+`shelf_life_days`+`expiry_date` 填两项自动算第三项，三项矛盾 → `400`；`status="finished"` 且未带 `finished_at` 时服务端自动填充当前本地日期 |
| `PUT /api/snacks/{id}` | 更新，`200`；不存在 `404`；日期校验同创建 |
| `DELETE /api/snacks/{id}` | `204`；不存在 `404` |
| `GET /api/locations` | 存放位置去重数组（字典序）；可选 `q` 做子串包含过滤，缺省/空串返回全部 |
| `GET /api/reminders` | Snack 字段 + `days_left` + `level`（`expired`\|`critical`\|`warning`），排除 `finished`，按过期日期升序 |
| `GET /api/users` `POST /api/users` | 仅管理员（非管理员 `403`）；创建 `201`，用户名重复 `409` |
| `DELETE /api/users/{id}` | `204`；删除自己 `400 {"error":"不能删除当前登录账户"}`；不存在 `404` |
| 非 `/api` 路径 | 托管静态文件；磁盘无对应文件时回退 `index.html`（SPA fallback） |

启动时会幂等迁移旧库：自动补 `production_date`/`shelf_life_days` 列；生命周期时间 `YYYY-MM-DDTHH:MM` 截取前 10 位为纯日期，非法值置 `NULL`；清理过期会话。

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
