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
export HOST="${HOST:-127.0.0.1}"
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
display_host="$HOST"
if [ "$display_host" = "0.0.0.0" ] || [ "$display_host" = "::" ]; then
  display_host="127.0.0.1"
fi
echo "[run] 访问地址：http://${display_host}:${PORT}"

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
