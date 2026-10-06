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

# 3) 后端构建
cd "$ROOT_DIR/backend"
cargo build

echo "编译成功"
echo "下一步：运行后端（默认 http://127.0.0.1:3000）："
echo "  SNACK_DB_PATH=data/app.db ./backend/target/debug/snack-warehouse-backend"
