#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# Docker 部署安装脚本 - snack-warehouse-management
# 部署单体镜像 snack-warehouse-management-app（前端+后端）
# ============================================================

# 彩色日志
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info()  { printf '%b[INFO]%b  %s\n' "$BLUE"   "$NC" "$*"; }
log_ok()    { printf '%b[ OK ]%b  %s\n' "$GREEN"  "$NC" "$*"; }
log_warn()  { printf '%b[WARN]%b  %s\n' "$YELLOW" "$NC" "$*"; }
log_error() { printf '%b[ERROR]%b %s\n' "$RED"    "$NC" "$*" >&2; }

# 默认值
PORT=3000
IMAGE="snack-warehouse-management-app:latest"
VOLUME="snack-data"
RESTART="unless-stopped"
CONTAINER_NAME="snack-warehouse"
COMPOSE_FILE="compose.yaml"
HEALTH_HOST="127.0.0.1"
HEALTH_WAIT_MAX=60
HEALTH_INTERVAL=3

usage() {
  cat <<EOF
用法: $0 [选项]

Docker 部署安装脚本 - 将单体镜像 snack-warehouse-management-app 部署为容器。

选项:
  --port PORT       宿主机映射端口（如 127.0.0.1:PORT:3000），默认 3000
  --image IMAGE     镜像标签，默认 snack-warehouse-management-app:latest
  --volume VOLUME   数据卷名，默认 snack-data
  --restart POLICY  容器重启策略，默认 unless-stopped
  -h, --help        显示此帮助信息

示例:
  $0 --port 8080
  $0 --image registry.example.com/snack-warehouse-management-app:latest --volume snack-data-prod
EOF
}

# ---------- 参数解析 ----------
while [[ $# -gt 0 ]]; do
  case "$1" in
    --port)
      PORT="$2"; shift 2 ;;
    --image)
      IMAGE="$2"; shift 2 ;;
    --volume)
      VOLUME="$2"; shift 2 ;;
    --restart)
      RESTART="$2"; shift 2 ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      log_error "未知参数: $1"
      usage
      exit 1
      ;;
  esac
done

# ---------- 基础校验 ----------
if ! [[ "$PORT" =~ ^[0-9]+$ ]] || [[ "$PORT" -lt 1 ]] || [[ "$PORT" -gt 65535 ]]; then
  log_error "无效端口: $PORT（必须是 1-65535 之间的整数）"
  exit 1
fi

# 脚本所在目录，确保 compose.yaml 在项目根目录生成
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# ---------- 环境检查：docker / compose ----------
if ! command -v docker >/dev/null 2>&1; then
  log_error "未找到 docker 命令，请先安装 Docker"
  exit 1
fi

if ! docker compose version >/dev/null 2>&1; then
  log_error "未找到 docker compose 插件（Compose V2），请先安装 Docker Compose"
  exit 1
fi

log_info "环境检查通过：docker + docker compose 可用"

# ---------- 检查镜像是否存在 ----------
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  log_error "镜像 $IMAGE 不存在，请先运行 ./docker-build.sh 构建镜像"
  exit 1
fi
log_ok "镜像已存在：$IMAGE"

# ---------- 检查同名运行中容器 ----------
if docker inspect -f '{{.State.Running}}' "$CONTAINER_NAME" 2>/dev/null | grep -q 'true'; then
  log_error "容器 $CONTAINER_NAME 已在运行中，请先运行 ./docker-update.sh 进行更新/重新部署"
  exit 1
fi

# ---------- 生成/覆盖 compose.yaml（保留 volume，数据持久化） ----------
cat > "$COMPOSE_FILE" <<EOF
version: '3.8'
services:
  app:
    image: ${IMAGE}
    container_name: ${CONTAINER_NAME}
    restart: ${RESTART}
    environment:
      - TZ=Asia/Shanghai
    ports:
      - '127.0.0.1:${PORT}:3000'
    volumes:
      - ${VOLUME}:/app/data
    healthcheck:
      test: ["CMD", "curl", "-kfsS", "https://127.0.0.1:3000/", "-o", "/dev/null"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 20s
    stop_grace_period: 15s
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL

volumes:
  ${VOLUME}:
    driver: local
EOF

log_info "已生成 $COMPOSE_FILE（端口 ${PORT}，镜像 ${IMAGE}，卷 ${VOLUME}，重启策略 ${RESTART}）"

# ---------- 启动容器 ----------
docker compose up -d --pull=never
log_ok "容器已启动（docker compose up -d --pull=never）"

# ---------- 等待健康检查（最多 60 秒，每 3 秒探测一次） ----------
log_info "等待服务健康检查（最多 ${HEALTH_WAIT_MAX} 秒，每 ${HEALTH_INTERVAL} 秒探测一次）..."
ELAPSED=0
HEALTHY=0
while [[ "$ELAPSED" -lt "$HEALTH_WAIT_MAX" ]]; do
  if curl -kfsS "https://${HEALTH_HOST}:${PORT}/" -o /dev/null 2>/dev/null; then
    HEALTHY=1
    break
  fi
  sleep "$HEALTH_INTERVAL"
  ELAPSED=$((ELAPSED + HEALTH_INTERVAL))
done

if [[ "$HEALTHY" -ne 1 ]]; then
  log_error "服务在 ${HEALTH_WAIT_MAX} 秒内未变为健康状态，请检查日志：docker compose logs"
  exit 1
fi
log_ok "服务已健康"

# ---------- 输出访问信息 ----------
echo
echo "============================================================"
log_ok "安装完成！"
echo "------------------------------------------------------------"
echo "  访问地址： https://${HEALTH_HOST}:${PORT}"
echo "  默认账号： admin"
echo "  默认密码： admin123"
echo "  容器名：   ${CONTAINER_NAME}"
echo "  数据卷：   ${VOLUME}"
echo "------------------------------------------------------------"
echo "注意：首次使用自签名 HTTPS 证书，浏览器会提示不受信任，"
echo "     选择继续访问即可。建议首次登录后立即修改默认密码。"
echo "============================================================"
