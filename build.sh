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

STATIC_DIR="$ROOT_DIR/backend/static"
rm -rf "$STATIC_DIR"
mkdir -p "$STATIC_DIR"
cp -R "$BROWSER_DIR/." "$STATIC_DIR/"

if [ ! -f "$STATIC_DIR/index.html" ]; then
  echo "错误：$STATIC_DIR/index.html 不存在，静态资源拷贝失败" >&2
  exit 1
fi
echo "[build] 静态资源已就绪：$STATIC_DIR (来源 $BROWSER_DIR)"

# 将运行/安装/卸载脚本直接生成到编译产物中，避免每次重新拷贝源脚本。
# 使用单引号定界符，使脚本内的变量为原文，不在 build.sh 执行时展开。

cat > "$STATIC_DIR/run.sh" <<'RUN_EOF'
#!/usr/bin/env bash
# 一键运行：必要时先自动编译（./build.sh），再以前台进程启动后端。
# Ctrl-C / SIGINT / SIGTERM 会转发给后端子进程并等待其退出，不留残留进程。
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_BIN="$ROOT_DIR/backend/target/debug/snack-warehouse-backend"
STATIC_INDEX="$ROOT_DIR/backend/static/index.html"

# 信号转发：把 INT/TERM 转给子进程所在的整个进程组（先于任何子步骤安装，
# 避免自动编译期间收到信号留下孤儿 build.sh/npm）
set -m  # 后台作业独立进程组，可整组转发/终止
child=""
forward() {
  if [ -n "$child" ] && kill -0 "$child" 2>/dev/null; then
    kill "-$1" -- "-$child" 2>/dev/null || kill "-$1" "$child" 2>/dev/null || true
  fi
}
trap 'forward INT' INT
trap 'forward TERM' TERM

# 1) 产物缺失（静态资源或后端二进制）→ 先自动编译（后台子进程，纳入信号转发）
if [ ! -f "$STATIC_INDEX" ] || [ ! -x "$BACKEND_BIN" ]; then
  echo "[run] 未检测到编译产物（$STATIC_INDEX / $BACKEND_BIN），先执行 ./build.sh …"
  "$ROOT_DIR/build.sh" &
  child=$!
  set +e
  wait "$child"
  build_status=$?
  set -e
  child=""
  if [ "$build_status" -ne 0 ]; then
    echo "[run] 自动编译未成功（退出码 $build_status），不启动后端"
    exit "$build_status"
  fi
fi

# 2) 环境变量默认值（沿用 backend/src/main.rs 语义：未设置/为空时取默认值）
export PORT="${PORT:-3000}"
export HOST="${HOST:-0.0.0.0}"
export STATIC_DIR="${STATIC_DIR:-$ROOT_DIR/backend/static}"
export SNACK_DB_PATH="${SNACK_DB_PATH:-data/app.db}"
export ADMIN_USERNAME="${ADMIN_USERNAME:-admin}"
export ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin123}"

# 相对路径（如默认 data/app.db）统一相对仓库根目录解析
cd "$ROOT_DIR"

echo "[run] 启动后端：$BACKEND_BIN"
echo "[run] 数据库：$SNACK_DB_PATH（STATIC_DIR=$STATIC_DIR）"
"$BACKEND_BIN" &
child=$!
if [ "$HOST" = "0.0.0.0" ] || [ "$HOST" = "::" ]; then
  # 通配地址：同时提示本机与局域网访问方式（IP 探测失败不影响运行）
  lan_ip=""
  if command -v hostname >/dev/null 2>&1; then
    lan_ip="$(hostname -I 2>/dev/null | awk '{print $1}')" || lan_ip=""
  fi
  if [ -z "$lan_ip" ] && command -v ip >/dev/null 2>&1; then
    lan_ip="$(ip -4 route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i=="src") print $(i+1); exit}')" || lan_ip=""
  fi
  echo "[run] 访问地址：http://127.0.0.1:${PORT}（本机）"
  if [ -n "$lan_ip" ]; then
    echo "[run] 局域网访问：http://${lan_ip}:${PORT}（其他设备；若无法访问请检查防火墙放行 ${PORT}/tcp）"
  else
    echo "[run] 局域网访问：http://<本机IP>:${PORT}（自动探测不可用；若无法访问请检查防火墙放行 ${PORT}/tcp）"
  fi
else
  echo "[run] 访问地址：http://${HOST}:${PORT}"
fi

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

cat > "$STATIC_DIR/install.sh" <<'INSTALL_EOF'
#!/usr/bin/env bash
# 一键安装：把零食仓库管理平台安装为 systemd 系统级守护进程。
#
# 用法：
#   sudo ./install.sh                 # 编译 + 安装/升级，开机自启
#   sudo ./install.sh --no-build      # 使用现有编译产物（不重新构建）
#   sudo ./install.sh --profile release   # 使用 release 版本后端二进制
#   sudo ./install.sh --help
#
# 安装位置（可用环境变量覆盖）：
#   INSTALL_DIR=/opt/snack-warehouse       程序与静态资源
#   DATA_DIR=/var/lib/snack-warehouse      数据库目录
#   ENV_FILE=/etc/snack-warehouse.env      环境变量配置（root 600）
#   SERVICE_NAME=snack-warehouse           systemd 服务名
#   RUN_USER=snack-warehouse               运行服务的系统用户
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SERVICE_NAME="${SERVICE_NAME:-snack-warehouse}"
INSTALL_DIR="${INSTALL_DIR:-/opt/snack-warehouse}"
DATA_DIR="${DATA_DIR:-/var/lib/snack-warehouse}"
ENV_FILE="${ENV_FILE:-/etc/snack-warehouse.env}"
RUN_USER="${RUN_USER:-snack-warehouse}"
UNIT_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
PROFILE="${PROFILE:-debug}"

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

if [ "$SKIP_BUILD" -eq 0 ]; then
  echo "[install] 编译前后端（./build.sh）…"
  run_builder "cd $(printf '%q' "$ROOT_DIR") && ./build.sh"
  if [ "$PROFILE" = "release" ]; then
    echo "[install] 编译 release 后端…"
    run_builder "cd $(printf '%q' "$ROOT_DIR/backend") && cargo build --release"
  fi
fi

BIN_SRC="$ROOT_DIR/backend/target/$PROFILE/snack-warehouse-backend"
STATIC_SRC="$ROOT_DIR/backend/static"
if [ ! -x "$BIN_SRC" ]; then
  echo "错误：找不到后端二进制 $BIN_SRC" >&2
  if [ "$PROFILE" = "release" ]; then
    echo "       请先运行 ./build.sh，再在 backend/ 下执行 cargo build --release；或去掉 --no-build。" >&2
  else
    echo "       请先运行 ./build.sh（或去掉 --no-build）。" >&2
  fi
  exit 1
fi

if [ ! -f "$STATIC_SRC/index.html" ]; then
  echo "错误：找不到前端静态资源 $STATIC_SRC/index.html" >&2
  echo "       请先运行 ./build.sh（或去掉 --no-build）。" >&2
  exit 1
fi

# 1) 服务运行用户（系统用户，无登录 shell）
if ! id -u "$RUN_USER" >/dev/null 2>&1; then
  NOLOGIN="$(command -v nologin || echo /usr/sbin/nologin)"
  echo "[install] 创建系统用户：$RUN_USER"
  useradd --system --home-dir "$DATA_DIR" --shell "$NOLOGIN" "$RUN_USER"
fi
if ! getent group "$RUN_USER" >/dev/null 2>&1; then
  echo "[install] 创建同名用户组：$RUN_USER"
  groupadd --system "$RUN_USER"
  usermod -aG "$RUN_USER" "$RUN_USER"
fi

# 2) 升级时先停旧服务
if systemctl is-active --quiet "$SERVICE_NAME" 2>/dev/null; then
  echo "[install] 停止正在运行的 $SERVICE_NAME.service …"
  systemctl stop "$SERVICE_NAME"
fi

# 3) 安装程序与静态资源（可重复执行，静态资源整体替换）
install -d -m 755 "$INSTALL_DIR"
install -m 755 "$BIN_SRC" "$INSTALL_DIR/snack-warehouse-backend"
rm -rf "$INSTALL_DIR/static"
install -d -m 755 "$INSTALL_DIR/static"
cp -a "$STATIC_SRC/." "$INSTALL_DIR/static/"
chmod -R a+rX "$INSTALL_DIR/static"
install -m 644 "$ROOT_DIR/README.md" "$INSTALL_DIR/README.md"

# 4) 数据目录（升级时确保归属正确）
install -d -m 750 "$DATA_DIR"
chown "$RUN_USER:$RUN_USER" "$DATA_DIR"
chmod 750 "$DATA_DIR"

# 5) 环境变量文件：首次安装生成，升级时保留用户修改
if [ ! -f "$ENV_FILE" ]; then
  echo "[install] 生成配置文件：$ENV_FILE"
  install -m 600 /dev/null "$ENV_FILE"
  cat > "$ENV_FILE" <<ENV_EOF
# 零食仓库管理平台 —— systemd 服务环境变量
# 修改后执行：sudo systemctl restart $SERVICE_NAME
HOST=0.0.0.0
PORT=3000
STATIC_DIR=$INSTALL_DIR/static
SNACK_DB_PATH=$DATA_DIR/app.db
ADMIN_USERNAME=admin
ADMIN_PASSWORD=admin123
ENV_EOF
  chmod 600 "$ENV_FILE"
  chown root:root "$ENV_FILE"
  echo "[install] ⚠️  默认管理员密码为 admin123，请登录后立即修改（或先编辑 $ENV_FILE 再重启服务）。"
else
  echo "[install] 保留已有配置文件：$ENV_FILE"
fi

# 6) systemd unit
# 若用户在配置文件中自定义了数据库路径，把其目录一并加入可写路径。
DB_PATH="$(sed -n 's/^[[:space:]]*SNACK_DB_PATH[[:space:]]*=[[:space:]]*//p' "$ENV_FILE" | tail -n 1)"
READ_WRITE_PATHS="$DATA_DIR"
case "$DB_PATH" in
  /*)
    DB_DIR="$(dirname "$DB_PATH")"
    if [ -n "$DB_DIR" ] && [ "$DB_DIR" != "$DATA_DIR" ]; then
      if [ ! -d "$DB_DIR" ]; then
        echo "[install] 警告：配置的数据库目录 $DB_DIR 不存在，服务可能启动失败。" >&2
      fi
      READ_WRITE_PATHS="$READ_WRITE_PATHS $DB_DIR"
    fi
    ;;
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
Group=$RUN_USER
WorkingDirectory=$INSTALL_DIR
EnvironmentFile=$ENV_FILE
ExecStart=$INSTALL_DIR/snack-warehouse-backend
Restart=on-failure
RestartSec=3

# 基础加固：程序目录只读，仅数据目录可写
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=$READ_WRITE_PATHS

[Install]
WantedBy=multi-user.target
UNIT_EOF
chmod 644 "$UNIT_FILE"

systemctl daemon-reload
systemctl enable "$SERVICE_NAME"
systemctl restart "$SERVICE_NAME"

# 7) 健康检查（从配置文件读取监听地址与端口）
HOST="$(sed -n 's/^[[:space:]]*HOST[[:space:]]*=[[:space:]]*//p' "$ENV_FILE" | tail -n 1)"
case "$HOST" in
  ""|0.0.0.0|::|\[::\]) HOST=127.0.0.1 ;;
esac
PORT="$(sed -n 's/^[[:space:]]*PORT[[:space:]]*=[[:space:]]*//p' "$ENV_FILE" | tail -n 1)"
PORT="${PORT:-3000}"
if command -v curl >/dev/null 2>&1; then
  ok=0
  for _ in $(seq 1 30); do
    if curl -fsS --max-time 2 "http://${HOST}:${PORT}/" >/dev/null 2>&1; then
      ok=1
      break
    fi
    sleep 1
  done
  if [ "$ok" -ne 1 ]; then
    echo "错误：服务已启动但健康检查失败（http://${HOST}:${PORT}/）。最近日志：" >&2
    systemctl --no-pager -l status "$SERVICE_NAME" | tail -n 20 >&2 || true
    exit 1
  fi
fi

echo
echo "安装完成 ✅"
echo "  服务：$SERVICE_NAME.service（已设为开机自启）"
echo "  程序：$INSTALL_DIR"
echo "  数据：$DATA_DIR/app.db"
echo "  配置：$ENV_FILE（root 600；修改后 systemctl restart $SERVICE_NAME）"
echo "  访问：http://${HOST}:${PORT}/（局域网用本机 IP，别忘了防火墙放行 ${PORT}/tcp）"
echo
echo "常用命令："
echo "  sudo systemctl status $SERVICE_NAME"
echo "  sudo systemctl restart $SERVICE_NAME"
echo "  sudo journalctl -u $SERVICE_NAME -f"
INSTALL_EOF

cat > "$STATIC_DIR/uninstall.sh" <<'UNINSTALL_EOF'
#!/usr/bin/env bash
# 一键卸载：移除由 install.sh 安装的 systemd 系统级服务。
#
# 用法：
#   sudo ./uninstall.sh                 # 卸载服务与程序，保留配置和数据库
#   sudo ./uninstall.sh --purge         # 连同配置、数据库、运行用户一并删除
#   sudo ./uninstall.sh --purge --yes   # 彻底删除且不再询问
#   sudo ./uninstall.sh --help
#
# 可通过环境变量覆盖安装时使用的路径（需与安装时一致）：
#   INSTALL_DIR=/opt/snack-warehouse
#   DATA_DIR=/var/lib/snack-warehouse
#   ENV_FILE=/etc/snack-warehouse.env
#   SERVICE_NAME=snack-warehouse
#   RUN_USER=snack-warehouse
set -euo pipefail

SERVICE_NAME="${SERVICE_NAME:-snack-warehouse}"
INSTALL_DIR="${INSTALL_DIR:-/opt/snack-warehouse}"
DATA_DIR="${DATA_DIR:-/var/lib/snack-warehouse}"
ENV_FILE="${ENV_FILE:-/etc/snack-warehouse.env}"
RUN_USER="${RUN_USER:-snack-warehouse}"
UNIT_FILE="/etc/systemd/system/${SERVICE_NAME}.service"

PURGE=0
ASSUME_YES=0

usage() {
  awk 'NR==1{next} /^#/{sub(/^# ?/,""); print; next} {exit}' "${BASH_SOURCE[0]}"
}

while [ $# -gt 0 ]; do
  case "$1" in
    --purge) PURGE=1 ;;
    -y|--yes) ASSUME_YES=1 ;;
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

if [ "$PURGE" -eq 1 ] && [ "$ASSUME_YES" -eq 0 ]; then
  echo "即将彻底删除以下内容："
  echo "  服务：$SERVICE_NAME.service"
  echo "  程序：$INSTALL_DIR"
  echo "  配置：$ENV_FILE"
  echo "  数据：$DATA_DIR（含数据库，不可恢复）"
  echo "  用户：$RUN_USER"
  printf '确认请输入 yes：'
  read -r answer || answer=""
  if [ "$answer" != "yes" ]; then
    echo "已取消。"
    exit 1
  fi
fi

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

# 2) 删除程序与静态资源
if [ -d "$INSTALL_DIR" ]; then
  echo "[uninstall] 删除程序目录：$INSTALL_DIR"
  rm -rf "$INSTALL_DIR"
fi

# 3) 可选：删除配置、数据与运行用户
if [ "$PURGE" -eq 1 ]; then
  if [ -f "$ENV_FILE" ]; then
    echo "[uninstall] 删除配置文件：$ENV_FILE"
    rm -f "$ENV_FILE"
  fi
  if [ -d "$DATA_DIR" ]; then
    echo "[uninstall] 删除数据目录：$DATA_DIR（含数据库）"
    rm -rf "$DATA_DIR"
  fi
  if [ "$RUN_USER" != "root" ] && id -u "$RUN_USER" >/dev/null 2>&1; then
    echo "[uninstall] 删除系统用户：$RUN_USER"
    userdel "$RUN_USER" 2>/dev/null || true
  fi
  if [ "$RUN_USER" != "root" ] && getent group "$RUN_USER" >/dev/null 2>&1; then
    groupdel "$RUN_USER" 2>/dev/null || true
  fi
  echo
  echo "卸载完成 ✅（服务、程序、配置、数据、用户均已删除）"
else
  echo
  echo "卸载完成 ✅"
  echo "已保留配置与数据（重新安装会继续使用）："
  echo "  配置：$ENV_FILE"
  echo "  数据：$DATA_DIR"
  echo "如需彻底删除，请执行：sudo ./uninstall.sh --purge"
fi
UNINSTALL_EOF

chmod 755 "$STATIC_DIR/run.sh" "$STATIC_DIR/install.sh" "$STATIC_DIR/uninstall.sh"
echo "[build] 部署脚本已生成：$STATIC_DIR/{run.sh,install.sh,uninstall.sh}"

# 3) 后端构建
cd "$ROOT_DIR/backend"
cargo build

echo "编译成功"
echo "下一步：运行 ./run.sh（默认 http://127.0.0.1:3000，Ctrl-C 停止）"
echo "  ./run.sh"
echo "部署脚本（已随产物输出）：$STATIC_DIR/{run.sh,install.sh,uninstall.sh}"
echo "手动方式（可选）：SNACK_DB_PATH=data/app.db ./backend/target/debug/snack-warehouse-backend"

# 3) 后端构建
cd "$ROOT_DIR/backend"
cargo build

echo "编译成功"
echo "下一步：运行 ./run.sh（默认 http://127.0.0.1:3000，Ctrl-C 停止）"
echo "  ./run.sh"
echo "手动方式（可选）：SNACK_DB_PATH=data/app.db ./backend/target/debug/snack-warehouse-backend"
