#!/usr/bin/env bash
# 一键运行：必要时先自动编译（./build.sh），再以前台进程启动后端。
# Ctrl-C / SIGINT / SIGTERM 会转发给后端子进程并等待其退出，不留残留进程。
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 自动定位编译产物：优先当前目录（dist/ 就地运行），回退到仓库布局
# 路径由脚本自动推导，不依赖 STATIC_DIR / SNACK_DB_PATH 等环境变量
if [ -x "$ROOT_DIR/snack-warehouse-backend" ]; then
  BACKEND_BIN="$ROOT_DIR/snack-warehouse-backend"
  STATIC_DIR="$ROOT_DIR/static"
else
  BACKEND_BIN="$ROOT_DIR/backend/target/debug/snack-warehouse-backend"
  STATIC_DIR="$ROOT_DIR/backend/static"
fi
STATIC_INDEX="$STATIC_DIR/index.html"

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

# 配置项可用环境变量覆盖（非路径）：PORT/HOST/ADMIN_USERNAME/ADMIN_PASSWORD
# 核心路径（STATIC_DIR / SNACK_DB_PATH）由脚本自动推导，不读环境变量
PORT="${PORT:-3000}"
HOST="${HOST:-0.0.0.0}"
ADMIN_USERNAME="${ADMIN_USERNAME:-admin}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin123}"
export PORT HOST ADMIN_USERNAME ADMIN_PASSWORD

# 相对路径（如默认 data/app.db）统一相对仓库根目录解析
cd "$ROOT_DIR"

DB_PATH="$ROOT_DIR/data/app.db"
echo "[run] 启动后端：$BACKEND_BIN"
echo "[run] 数据库：$DB_PATH（STATIC_DIR=$STATIC_DIR）"
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
