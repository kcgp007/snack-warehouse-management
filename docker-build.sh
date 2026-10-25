#!/usr/bin/env bash
set -euo pipefail

# ─────────────────────────────────────────────────────────────
# docker-build.sh — 零食仓库管理平台 Docker 镜像构建脚本
# 构建 snack-warehouse-management-app:latest 并打 YYYYMMDD 版本标签
# ─────────────────────────────────────────────────────────────

# 彩色日志
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

INFO()  { printf "${BLUE}[INFO]${NC}  %s\n" "$*"; }
ERROR() { printf "${RED}[ERROR]${NC} %s\n" "$*" >&2; }

usage() {
    cat <<EOF
用法: ${0##*/} [选项]

在项目根目录执行 docker compose build，构建零食仓库管理平台镜像，
并同时打一个带日期标签（snack-warehouse-management-app:YYYYMMDD）。

选项:
  -h, --help    显示此帮助信息并退出

示例:
  ./${0##*/}
  ./${0##*/} --help
EOF
}

# 解析参数
while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        *)
            ERROR "未知参数: $1"
            echo
            usage
            exit 1
            ;;
    esac
done

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKERFILE="$PROJECT_ROOT/Dockerfile"
IMAGE_NAME="snack-warehouse-management-app"
DATE_TAG="$(date +%Y%m%d-%H%M%S)"

INFO "项目根目录: $PROJECT_ROOT"

# 6. 执行前检查 Dockerfile 是否存在
if [[ ! -f "$DOCKERFILE" ]]; then
    ERROR "未找到 Dockerfile: $DOCKERFILE，请确认在项目根目录运行本脚本。"
    exit 1
fi

# 5. 假设用户已安装 docker 与 docker compose，仅做可用性探测
if ! command -v docker >/dev/null 2>&1; then
    ERROR "未找到 docker 命令，请先安装 Docker。"
    exit 1
fi
if ! docker compose version >/dev/null 2>&1; then
    ERROR "未找到 docker compose 插件，请先安装 Docker Compose。"
    exit 1
fi

cd "$PROJECT_ROOT"

# 1. 构建镜像 snack-warehouse-management-app:latest
INFO "开始构建镜像: $IMAGE_NAME:latest"
if ! docker compose build; then
    ERROR "docker compose build 失败，请查看上方错误信息。"
    exit 1
fi

# 定位到构建出的镜像 ID 与大小
IMAGE_ID="$(docker inspect --format='{{.Id}}' "$IMAGE_NAME:latest" 2>/dev/null || true)"
if [[ -z "$IMAGE_ID" ]]; then
    ERROR "未能定位到镜像 $IMAGE_NAME:latest，构建可能未产出该标签。"
    exit 1
fi

IMAGE_SIZE_BYTES="$(docker inspect --format='{{.Size}}' "$IMAGE_NAME:latest" 2>/dev/null || true)"
if [[ -z "$IMAGE_SIZE_BYTES" ]]; then
    ERROR "未能获取镜像 $IMAGE_NAME:latest 的大小，镜像可能不存在。"
    exit 1
fi

# 人类可读的大小（优先 numfmt，不可用则回退原始字节）
if command -v numfmt >/dev/null 2>&1; then
    IMAGE_SIZE="$(numfmt --to=iec --suffix=B --format="%.2f" "$IMAGE_SIZE_BYTES" 2>/dev/null || echo "${IMAGE_SIZE_BYTES}B")"
else
    IMAGE_SIZE="${IMAGE_SIZE_BYTES}B"
fi

# 同时打带日期/版本标签
docker tag "$IMAGE_ID" "$IMAGE_NAME:$DATE_TAG"

# 2. 构建成功后输出镜像大小与 ID，并提示下一步部署
INFO "镜像 ID:    $IMAGE_ID"
INFO "镜像大小:   $IMAGE_SIZE"
INFO "标签 latest: $IMAGE_NAME:latest"
INFO "标签 日期:   $IMAGE_NAME:$DATE_TAG"

echo
echo -e "${GREEN}══════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  构建完成！${NC}"
echo -e "${GREEN}══════════════════════════════════════════════════════${NC}"
echo "下一步部署：运行 ./docker-install.sh 部署并启动容器。"
