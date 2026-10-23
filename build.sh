#!/usr/bin/env bash
# 一键编译：前端 Angular 构建 → 产物拷贝到 backend/static → 后端 cargo build。
# 可重复执行（每次都会重新生成 backend/static）。
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 1) 前端构建
cd "$ROOT_DIR/frontend"
npm install
npm run build

# 2) 定位含 index.html 的构建产物目录（常见 frontend/dist/<app>/browser）
BROWSER_DIR="$(find "$ROOT_DIR/frontend/dist" -type f -name index.html -printf '%h\n' | head -n 1)"
if [ -z "$BROWSER_DIR" ]; then
  echo "错误：未在 frontend/dist 下找到 index.html，前端构建产物异常" >&2
  exit 1
fi

DIST_DIR="$ROOT_DIR/dist"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR/static"
cp -R "$BROWSER_DIR/." "$DIST_DIR/static/"

if [ ! -f "$DIST_DIR/static/index.html" ]; then
  echo "错误：$DIST_DIR/static/index.html 不存在，静态资源拷贝失败" >&2
  exit 1
fi
echo "[build] 静态资源已就绪：$DIST_DIR/static (来源 $BROWSER_DIR)"

# 将运行/安装/卸载脚本直接生成到编译产物中，避免每次重新拷贝源脚本。
# 使用单引号定界符，使脚本内的变量为原文，不在 build.sh 执行时展开。

cat > "$DIST_DIR/run.sh" <<'RUN_EOF'
#!/usr/bin/env bash
# 一键运行：以前台进程启动后端（需先执行 build.sh 生成 dist/）。
# Ctrl-C / SIGINT / SIGTERM 会转发给后端子进程并等待其退出，不留残留进程。
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# 产物与脚本同目录（build.sh 生成的 dist/ 布局）；路径自动推导，不依赖环境变量
BACKEND_BIN="$ROOT_DIR/snack-warehouse-backend"
STATIC_DIR="$ROOT_DIR/static"
STATIC_INDEX="$STATIC_DIR/index.html"

if [ ! -x "$BACKEND_BIN" ] || [ ! -f "$STATIC_INDEX" ]; then
  echo "[run] 缺少编译产物（$BACKEND_BIN / $STATIC_INDEX）" >&2
  echo "[run] 请在仓库根目录执行 ./build.sh 重新生成 dist/ 后再运行" >&2
  exit 1
fi

# 信号转发：把 INT/TERM 转给后端子进程
set -m  # 后台作业独立进程组，可整组转发/终止
child=""
forward() {
  if [ -n "$child" ] && kill -0 "$child" 2>/dev/null; then
    kill "-$1" -- "-$child" 2>/dev/null || kill "-$1" "$child" 2>/dev/null || true
  fi
}
trap 'forward INT' INT
trap 'forward TERM' TERM

# 配置：运行目录下的 snack-warehouse.toml（与二进制同级），由后端自行读取；
# 本脚本不再导出环境变量，核心路径由脚本自动推导。
cd "$ROOT_DIR"

echo "[run] 启动后端：$BACKEND_BIN"
echo "[run] 数据库：$ROOT_DIR/data/app.db（静态资源：$STATIC_DIR）"
echo "[run] 运行目录：$ROOT_DIR"
echo "[run] 配置：运行目录下的 snack-warehouse.toml（缺失则使用默认值 host=0.0.0.0 port=3000 admin=admin/admin123）"
"$BACKEND_BIN" &
child=$!
echo "[run] 访问地址：https://127.0.0.1:3000（本机；自签名证书首次访问需在浏览器手动信任）"
echo "[run] 局域网访问：请在浏览器中输入 https://<本机IP>:3000（若无法访问请检查防火墙放行 3000/tcp）"

# 4) 等待子进程退出；若 wait 被信号陷阱打断（子进程仍在运行）则继续等待
set +e
status=0
while true; do
  wait "$child"
  status=$?
  kill -0 "$child" 2>/dev/null || break
done
set -e

echo "[run] 后端已退出（退出码 $status）"
exit "$status"
RUN_EOF

cat > "$DIST_DIR/install.sh" <<'INSTALL_EOF'
#!/usr/bin/env bash
# 一键安装：把零食仓库管理平台安装为 systemd 系统级守护进程。
#
# 用法：
#   sudo ./install.sh                 # 编译 + 安装/升级，开机自启
#   sudo ./install.sh --no-build      # 使用现有编译产物（不重新构建）
#   sudo ./install.sh --profile release   # 使用 release 版本后端二进制
#   sudo ./install.sh --help
#
# 安装位置（由脚本所在目录确定，不可通过环境变量覆盖）：
#   程序与静态资源：install.sh所在目录
#   数据库目录：install.sh所在目录下的 data/
#   配置文件：install.sh所在目录下的 snack-warehouse.toml
#   SERVICE_NAME=snack-warehouse           systemd 服务名
#   RUN_USER=snack-warehouse               运行服务的系统用户
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SERVICE_NAME=snack-warehouse
INSTALL_DIR="$ROOT_DIR"  # 由脚本所在目录确定，不可通过环境变量覆盖
DATA_DIR="$ROOT_DIR/data"
# 配置文件：运行目录下的 snack-warehouse.toml（非 /etc 目录下的环境变量文件）
CONFIG_FILE="$INSTALL_DIR/snack-warehouse.toml"
UNIT_FILE="/etc/systemd/system/${SERVICE_NAME}.service"

# 服务运行用户：
#   - 安装在 /home 下时，家目录通常不允许其他用户穿越（会报 200/CHDIR Permission denied），
#     因此直接以安装目录的所有者身份运行（不创建、也不删除该用户）。
#   - 其他位置使用专用系统用户 snack-warehouse。
if [[ "$INSTALL_DIR" == /home/* ]]; then
  INSTALL_OWNER="$(stat -c '%U' "$INSTALL_DIR")"
  if [ "$INSTALL_OWNER" = "root" ] || [ "$INSTALL_OWNER" = "UNKNOWN" ]; then
    echo "错误：$INSTALL_DIR 属于 root，无法以非 root 身份运行。" >&2
    echo "       请先 chown 给普通用户，或把整个 dist/ 安装到 /opt 等目录后重试。" >&2
    exit 1
  fi
  RUN_USER="$INSTALL_OWNER"
  RUN_GROUP="$(id -gn "$RUN_USER")"
  MANAGE_USER=0
else
  RUN_USER=snack-warehouse
  RUN_GROUP=snack-warehouse
  MANAGE_USER=1
fi
PROFILE=debug

SKIP_BUILD=0

usage() {
  awk 'NR==1{next} /^#/{sub(/^# ?/,""); print; next} {exit}' "${BASH_SOURCE[0]}"
}

while [ $# -gt 0 ]; do
  case "$1" in
    --no-build) SKIP_BUILD=1 ;;
    --profile)
      shift
      PROFILE="${1:-}"
      ;;
    --profile=*) PROFILE="${1#*=}" ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "未知参数：$1（使用 --help 查看用法）" >&2
      exit 2
      ;;
  esac
  shift
done

if [ "$PROFILE" != "debug" ] && [ "$PROFILE" != "release" ]; then
  echo "错误：--profile 只能是 debug 或 release" >&2
  exit 2
fi

if ! [[ "$SERVICE_NAME" =~ ^[A-Za-z0-9_.@-]+$ ]]; then
  echo "错误：SERVICE_NAME 只能包含字母、数字、._@-：$SERVICE_NAME" >&2
  exit 1
fi
if ! [[ "$RUN_USER" =~ ^[a-z_][a-z0-9_-]*$ ]]; then
  echo "错误：RUN_USER 不是合法的系统用户名：$RUN_USER" >&2
  exit 1
fi
if [ "$RUN_USER" = "root" ]; then
  echo "错误：拒绝以 root 用户运行服务，请使用专用系统用户（默认 snack-warehouse）。" >&2
  exit 1
fi
for path in "$INSTALL_DIR" "$DATA_DIR"; do
  case "$path" in
    /|/opt|/var|/var/lib|/usr|/etc|/home|/root|"")
      echo "错误：拒绝使用危险路径：$path" >&2
      exit 1
      ;;
    *[[:space:]\'\"%]*)
      echo "错误：路径不能包含空白、引号或 %（systemd 不支持）：$path" >&2
      exit 1
      ;;
  esac
done

if [ "$(id -u)" -ne 0 ]; then
  echo "错误：安装系统级服务需要 root 权限，请使用：sudo ./install.sh" >&2
  exit 1
fi

if [ ! -d /run/systemd/system ]; then
  echo "错误：当前系统未使用 systemd（/run/systemd/system 不存在），无法安装系统服务。" >&2
  exit 1
fi

if ! command -v systemctl >/dev/null 2>&1; then
  echo "错误：找不到 systemctl，无法安装系统服务。" >&2
  exit 1
fi

# 构建要以仓库所有者的身份执行，避免在 target/ 下留下 root 所有的文件。
# 使用登录 shell，确保 rustup/nvm 等用户级工具链在 PATH 中。
run_builder() {
  local cmd="$1"
  if [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
    sudo -u "$SUDO_USER" -H bash -lc "$cmd"
  else
    bash -lc "$cmd"
  fi
}

# 定位产物：优先脚本所在目录（dist 就地运行），再回退到源码仓库布局
if [ -x "$ROOT_DIR/snack-warehouse-backend" ]; then
  BIN_SRC="$ROOT_DIR/snack-warehouse-backend"
  BUILD_LAYOUT=dist
else
  BIN_SRC="$ROOT_DIR/backend/target/$PROFILE/snack-warehouse-backend"
  BUILD_LAYOUT=repo
fi
if [ -d "$ROOT_DIR/static" ]; then
  STATIC_SRC="$ROOT_DIR/static"
else
  STATIC_SRC="$ROOT_DIR/backend/static"
fi

# 仅在源码仓库布局下才需要编译；dist 就地运行时二进制与静态资源已齐全
if [ "$BUILD_LAYOUT" = "repo" ] && [ "$SKIP_BUILD" -eq 0 ]; then
  echo "[install] 编译前后端（./build.sh）…"
  run_builder "$ROOT_DIR/build.sh"
  # 编译后重新定位（dist 可能被 build.sh 重新生成）
  if [ -x "$ROOT_DIR/dist/snack-warehouse-backend" ]; then
    BIN_SRC="$ROOT_DIR/dist/snack-warehouse-backend"
    STATIC_SRC="$ROOT_DIR/dist/static"
    BUILD_LAYOUT=dist
    ROOT_DIR="$ROOT_DIR/dist"
  fi
  if [ "$PROFILE" = "release" ]; then
    echo "[install] 编译 release 后端…"
    run_builder "cd $(printf '%q' "$ROOT_DIR")/../backend && cargo build --release"
  fi
fi

if [ ! -x "$BIN_SRC" ]; then
  echo "错误：找不到后端二进制 $BIN_SRC" >&2
  echo "       请先运行 ./build.sh 生成产物，或将本脚本置于 dist/ 目录中就地运行。" >&2
  exit 1
fi

if [ ! -f "$STATIC_SRC/index.html" ]; then
  echo "错误：找不到前端静态资源 $STATIC_SRC/index.html" >&2
  echo "       请先运行 ./build.sh 生成产物。" >&2
  exit 1
fi

# 1) 服务运行用户
if [ "$MANAGE_USER" -eq 1 ]; then
  # 专用系统用户：无登录 shell，仅用于运行服务
  if ! id -u "$RUN_USER" >/dev/null 2>&1; then
    NOLOGIN="$(command -v nologin || echo /usr/sbin/nologin)"
    echo "[install] 创建系统用户：$RUN_USER"
    useradd --system --home-dir "$DATA_DIR" --shell "$NOLOGIN" "$RUN_USER"
  fi
  if ! getent group "$RUN_GROUP" >/dev/null 2>&1; then
    echo "[install] 创建同名用户组：$RUN_GROUP"
    groupadd --system "$RUN_GROUP"
    usermod -aG "$RUN_GROUP" "$RUN_USER"
  fi
else
  echo "[install] 安装目录位于家目录下：以目录所有者 $RUN_USER（组 $RUN_GROUP）身份运行"
  if id -u snack-warehouse >/dev/null 2>&1; then
    echo "[install] 提示：检测到旧安装创建的 snack-warehouse 系统用户，本次安装不再使用。"
    echo "          确认无用后可执行：sudo userdel snack-warehouse"
  fi
fi

# 2) 升级时先停旧服务
if systemctl is-active --quiet "$SERVICE_NAME" 2>/dev/null; then
  echo "[install] 停止正在运行的 $SERVICE_NAME.service …"
  systemctl stop "$SERVICE_NAME"
fi

# 3) 安装程序与静态资源（可重复执行，静态资源整体替换）
# 就地运行（INSTALL_DIR == 脚本所在目录）时跳过自复制，避免 rm -rf 误删源文件
if [ "$(cd "$INSTALL_DIR" && pwd)" = "$(cd "$ROOT_DIR" && pwd)" ]; then
  echo "[install] 就地运行：$INSTALL_DIR（产物已就位，跳过自复制）"
else
  install -d -m 755 "$INSTALL_DIR"
  install -m 755 "$BIN_SRC" "$INSTALL_DIR/snack-warehouse-backend"
  rm -rf "$INSTALL_DIR/static"
  install -d -m 755 "$INSTALL_DIR/static"
  cp -a "$STATIC_SRC/." "$INSTALL_DIR/static/"
  chmod -R a+rX "$INSTALL_DIR/static"
  if [ -f "$ROOT_DIR/README.md" ]; then
    install -m 644 "$ROOT_DIR/README.md" "$INSTALL_DIR/README.md"
  fi
fi

# 4) 数据目录（升级时确保归属正确）
install -d -m 750 "$DATA_DIR"
chown "$RUN_USER:$RUN_GROUP" "$DATA_DIR"
chmod 750 "$DATA_DIR"

# 5) 配置文件（TOML，运行目录下）：首次安装生成，升级时保留用户修改
if [ ! -f "$CONFIG_FILE" ]; then
  echo "[install] 生成配置文件：$CONFIG_FILE（TOML 格式，运行目录下）"
  install -m 640 /dev/null "$CONFIG_FILE"
  cat > "$CONFIG_FILE" <<TOML_EOF
host = "0.0.0.0"
port = 3000
admin_username = "admin"
admin_password = "admin123"
TOML_EOF
  echo "[install] ⚠️  默认管理员密码为 admin123，请登录后立即修改（或先编辑 $CONFIG_FILE 再重启服务）。"
else
  echo "[install] 保留已有配置文件：$CONFIG_FILE（TOML 格式，运行目录下）"
fi
# 无论新建还是升级：root 可写、服务组只读，避免服务读不到配置
chown root:"$RUN_GROUP" "$CONFIG_FILE"
chmod 640 "$CONFIG_FILE"

# 6) systemd unit
READ_WRITE_PATHS="$DATA_DIR"
# 安装在 /home 下时，ProtectHome=true 会屏蔽程序目录，导致服务无法启动（表现为健康检查失败）
case "$INSTALL_DIR" in
  /home/*) PROTECT_HOME="read-only" ;;
  *) PROTECT_HOME="true" ;;
esac

echo "[install] 写入 systemd 服务：$UNIT_FILE"
cat > "$UNIT_FILE" <<UNIT_EOF
[Unit]
Description=Snack Warehouse Management
Documentation=file://$INSTALL_DIR/README.md
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=$RUN_USER
Group=$RUN_GROUP
WorkingDirectory=$INSTALL_DIR
# 配置由运行目录下的 TOML 文件读取，不再使用 EnvironmentFile
# （后端 main.rs 从运行目录的 snack-warehouse.toml 读取配置）
ExecStart=$INSTALL_DIR/snack-warehouse-backend
Restart=on-failure
RestartSec=3

# 基础加固：程序目录只读，仅数据目录可写
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=$PROTECT_HOME
ReadWritePaths=$READ_WRITE_PATHS

[Install]
WantedBy=multi-user.target
UNIT_EOF
chmod 644 "$UNIT_FILE"

systemctl daemon-reload
systemctl enable "$SERVICE_NAME"
systemctl restart "$SERVICE_NAME"

# 7) 健康检查（从配置文件读取实际监听地址与端口）
toml_get() {
  awk -F= -v key="$1" '
    $1 ~ "^[[:space:]]*" key "[[:space:]]*$" {
      v = $2
      sub(/#.*/, "", v)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", v)
      gsub(/"/, "", v)
      print v
      exit
    }' "$CONFIG_FILE" 2>/dev/null || true
}
HOST="$(toml_get host)"
HOST="${HOST:-0.0.0.0}"
PORT="$(toml_get port)"
PORT="${PORT:-3000}"
case "$HOST" in
  ""|0.0.0.0|::|\[::\]) HOST=127.0.0.1 ;;
esac
case "$HOST" in
  *:*) CURL_HOST="[$HOST]" ;;
  *) CURL_HOST="$HOST" ;;
esac
if command -v curl >/dev/null 2>&1; then
  ok=0
  for _ in $(seq 1 30); do
    if curl -kfsS --max-time 2 "https://${CURL_HOST}:${PORT}/" >/dev/null 2>&1; then
      ok=1
      break
    fi
    sleep 1
  done
  if [ "$ok" -ne 1 ]; then
    echo "错误：健康检查失败（https://${CURL_HOST}:${PORT}/，等待 30 秒）。" >&2
    echo "--- systemctl status ---" >&2
    systemctl --no-pager -l status "$SERVICE_NAME" 2>&1 | tail -n 20 >&2 || true
    echo "--- journalctl（最近 50 行） ---" >&2
    journalctl -u "$SERVICE_NAME" -n 50 --no-pager 2>&1 | tail -n 50 >&2 || true
    echo >&2
    echo "排查提示：" >&2
    echo "  - 端口被占用：ss -ltnp | grep :${PORT}" >&2
    echo "  - 配置无法读取：ls -l $CONFIG_FILE（应为 root:$RUN_GROUP 640）" >&2
    echo "  - 手动前台运行：cd $INSTALL_DIR && sudo -u $RUN_USER ./snack-warehouse-backend" >&2
    exit 1
  fi
fi

echo
echo "安装完成 ✅"
echo "  服务：$SERVICE_NAME.service（已设为开机自启）"
echo "  程序：$INSTALL_DIR"
echo "  数据：$DATA_DIR/app.db"
echo "  配置：$CONFIG_FILE（root:$RUN_GROUP 640；修改后 systemctl restart $SERVICE_NAME）"
echo "  访问：https://${HOST}:${PORT}/（局域网用本机 IP，别忘了防火墙放行 ${PORT}/tcp；自签名证书需手动信任）"
echo
echo "常用命令："
echo "  sudo systemctl status $SERVICE_NAME"
echo "  sudo systemctl restart $SERVICE_NAME"
echo "  sudo journalctl -u $SERVICE_NAME -f"
INSTALL_EOF

cat > "$DIST_DIR/uninstall.sh" <<'UNINSTALL_EOF'
#!/usr/bin/env bash
# 一键卸载：移除由 install.sh 安装的 systemd 系统级服务。
#
# 用法：
#   sudo ./uninstall.sh            # 交互式卸载（会询问是否删除数据与运行用户）
#   sudo ./uninstall.sh --help
#
# 卸载始终移除：systemd 服务、unit 文件、程序与静态资源。
# 数据库/证书（data/）、配置文件与系统用户默认保留，仅在交互提示中输入 y 才会删除。
#
# 安装位置（由脚本所在目录确定，不可通过环境变量覆盖）：
#   程序与静态资源：install.sh所在目录
#   数据库目录：install.sh所在目录下的 data/
#   配置文件：install.sh所在目录下的 snack-warehouse.toml
#   SERVICE_NAME=snack-warehouse           systemd 服务名
#   RUN_USER=snack-warehouse               运行服务的系统用户
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SERVICE_NAME=snack-warehouse
INSTALL_DIR="$ROOT_DIR"  # 由脚本所在目录确定，不可通过环境变量覆盖
DATA_DIR="$ROOT_DIR/data"
# 配置文件：运行目录下的 snack-warehouse.toml（非 /etc 目录下的环境变量文件）
CONFIG_FILE="$INSTALL_DIR/snack-warehouse.toml"
UNIT_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
# 从已安装的 unit 读取实际运行用户（安装在家目录时是普通用户，卸载绝不能删除该账户）
INSTALLED_USER="$(awk -F= '/^[[:space:]]*User=/{gsub(/[[:space:]]/, "", $2); print $2; exit}' "$UNIT_FILE" 2>/dev/null || true)"
INSTALLED_GROUP="$(awk -F= '/^[[:space:]]*Group=/{gsub(/[[:space:]]/, "", $2); print $2; exit}' "$UNIT_FILE" 2>/dev/null || true)"
RUN_USER="${INSTALLED_USER:-snack-warehouse}"
RUN_GROUP="${INSTALLED_GROUP:-$RUN_USER}"

usage() {
  awk 'NR==1{next} /^#/{sub(/^# ?/,""); print; next} {exit}' "${BASH_SOURCE[0]}"
}

# 交互式确认：仅输入 y/yes（不区分大小写）视为同意，其余（含直接回车、EOF）均为 N
ask_yes_no() {
  local prompt="$1" answer=""
  printf '%s [y/N] ' "$prompt"
  if ! read -r answer; then
    echo
    return 1
  fi
  case "${answer,,}" in
    y|yes) return 0 ;;
    *) return 1 ;;
  esac
}

while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "未知参数：$1（使用 --help 查看用法；删除数据与用户请在交互提示中输入 y）" >&2
      exit 2
      ;;
  esac
  shift
done

if [ "$(id -u)" -ne 0 ]; then
  echo "错误：卸载系统级服务需要 root 权限，请使用：sudo ./uninstall.sh" >&2
  exit 1
fi

if ! [[ "$SERVICE_NAME" =~ ^[A-Za-z0-9_.@-]+$ ]]; then
  echo "错误：SERVICE_NAME 只能包含字母、数字、._@-：$SERVICE_NAME" >&2
  exit 1
fi
for path in "$INSTALL_DIR" "$DATA_DIR"; do
  case "$path" in
    /|/opt|/var|/var/lib|/usr|/etc|/home|/root|"")
      echo "错误：拒绝使用危险路径：$path" >&2
      exit 1
      ;;
  esac
done

echo "即将卸载以下内容（不可撤销）："
echo "  服务：$SERVICE_NAME.service"
echo "  unit：$UNIT_FILE"
echo "  程序与静态资源：$INSTALL_DIR"
echo
DELETE_DATA=0
DELETE_USER=0
if ask_yes_no "是否同时删除数据库与配置？$DATA_DIR 与 $CONFIG_FILE（不可恢复）"; then
  DELETE_DATA=1
fi
DELETE_USER=0
if [ "$RUN_USER" != "root" ] && id -u "$RUN_USER" >/dev/null 2>&1; then
  if [ "$(id -u "$RUN_USER")" -lt 1000 ]; then
    if ask_yes_no "是否同时删除系统用户与用户组 $RUN_USER？"; then
      DELETE_USER=1
    fi
  else
    echo "提示：运行用户 $RUN_USER 是普通账户，卸载不会删除该用户。"
  fi
fi
echo

# 1) 停止并移除 systemd 服务
if command -v systemctl >/dev/null 2>&1; then
  if systemctl is-active --quiet "$SERVICE_NAME" 2>/dev/null; then
    echo "[uninstall] 停止 $SERVICE_NAME.service …"
    systemctl stop "$SERVICE_NAME"
  fi
  if systemctl is-enabled --quiet "$SERVICE_NAME" 2>/dev/null; then
    echo "[uninstall] 取消开机自启 …"
    systemctl disable "$SERVICE_NAME"
  fi
fi

if [ -f "$UNIT_FILE" ]; then
  echo "[uninstall] 删除 unit 文件：$UNIT_FILE"
  rm -f "$UNIT_FILE"
fi

if command -v systemctl >/dev/null 2>&1; then
  systemctl daemon-reload 2>/dev/null || true
  systemctl reset-failed "$SERVICE_NAME" 2>/dev/null || true
fi

# 2) 删除程序与静态资源（data/ 与 snack-warehouse.toml 暂留，按交互结果决定是否删除）
if [ -d "$INSTALL_DIR" ]; then
  echo "[uninstall] 删除程序与静态资源：$INSTALL_DIR"
  find "$INSTALL_DIR" -mindepth 1 -maxdepth 1 \
    ! -name 'data' ! -name 'snack-warehouse.toml' \
    -exec rm -rf -- {} +
fi

# 3) 按交互选择删除配置与数据
if [ "$DELETE_DATA" -eq 1 ]; then
  if [ -f "$CONFIG_FILE" ]; then
    echo "[uninstall] 删除配置文件：$CONFIG_FILE"
    rm -f "$CONFIG_FILE"
  fi
  if [ -d "$DATA_DIR" ]; then
    echo "[uninstall] 删除数据目录：$DATA_DIR（含数据库与证书）"
    rm -rf "$DATA_DIR"
  fi
fi

# 4) 按交互选择删除系统用户与用户组
if [ "$DELETE_USER" -eq 1 ]; then
  if [ "$RUN_USER" != "root" ] && id -u "$RUN_USER" >/dev/null 2>&1 && [ "$(id -u "$RUN_USER")" -lt 1000 ]; then
    echo "[uninstall] 删除系统用户：$RUN_USER"
    userdel "$RUN_USER" 2>/dev/null || true
  fi
  # 仅删除系统组（gid < 1000），避免误删普通用户的主组
  if [ "$RUN_GROUP" != "root" ] && getent group "$RUN_GROUP" >/dev/null 2>&1 \
     && [ "$(getent group "$RUN_GROUP" | cut -d: -f3)" -lt 1000 ] 2>/dev/null; then
    echo "[uninstall] 删除系统用户组：$RUN_GROUP"
    groupdel "$RUN_GROUP" 2>/dev/null || true
  fi
fi

# 已清空则顺手删除安装目录本身
rmdir "$INSTALL_DIR" 2>/dev/null || true

echo
echo "卸载完成 ✅"
KEEP=()
if [ "$DELETE_DATA" -eq 0 ]; then
  [ -f "$CONFIG_FILE" ] && KEEP+=("配置：$CONFIG_FILE")
  [ -d "$DATA_DIR" ] && KEEP+=("数据：$DATA_DIR")
fi
[ "$DELETE_USER" -eq 0 ] && id -u "$RUN_USER" >/dev/null 2>&1 \
  && [ "$(id -u "$RUN_USER")" -lt 1000 ] && KEEP+=("用户：$RUN_USER")
if [ "${#KEEP[@]}" -gt 0 ]; then
  echo "已保留（重新安装会继续使用）："
  printf '  %s\n' "${KEEP[@]}"
else
  echo "配置、数据与运行用户均已删除。"
fi
UNINSTALL_EOF

chmod 755 "$DIST_DIR/run.sh" "$DIST_DIR/install.sh" "$DIST_DIR/uninstall.sh"
echo "[build] 部署脚本已生成：$DIST_DIR/{run.sh,install.sh,uninstall.sh}"

# 3) 后端构建
cd "$ROOT_DIR/backend"
cargo build

# 4) 复制后端二进制到 dist/
BACKEND_BIN_SRC="$ROOT_DIR/backend/target/debug/snack-warehouse-backend"
if [ ! -x "$BACKEND_BIN_SRC" ]; then
  echo "错误：找不到后端二进制 $BACKEND_BIN_SRC" >&2
  exit 1
fi
cp -f "$BACKEND_BIN_SRC" "$DIST_DIR/snack-warehouse-backend"
chmod 755 "$DIST_DIR/snack-warehouse-backend"
echo "[build] 后端二进制已就绪：$DIST_DIR/snack-warehouse-backend"

echo "编译成功，产物输出目录：$DIST_DIR/"
echo "下一步：进入 $DIST_DIR/ 运行 ./run.sh（默认 https://127.0.0.1:3000，Ctrl-C 停止）"
echo "  cd $DIST_DIR && ./run.sh"
echo "部署脚本：$DIST_DIR/{run.sh,install.sh,uninstall.sh}"
echo "手动方式（可选）：cd $DIST_DIR && ./snack-warehouse-backend"
