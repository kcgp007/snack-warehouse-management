#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# Docker 卸载脚本 — snack-warehouse-management
# 用于卸载通过 compose.yaml 部署的 snack-warehouse 容器
# ============================================================

# ---------- 颜色日志 ----------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

log_info()  { echo -e "${BLUE}[INFO]${NC}  $*"; }
log_success() { echo -e "${GREEN}[OK]${NC}   $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERR]${NC}   $*" >&2; }
log_prompt() { echo -ne "${CYAN}[?]${NC}  $*"; }

# ---------- 默认配置 ----------
VOLUME_NAME="snack-data"
CONTAINER_NAME="snack-warehouse"
IMAGE_NAME="snack-warehouse-management-app"
COMPOSE_FILE="compose.yaml"
KEEP_DATA=true
YES=false

DELETED_ITEMS=()
KEPT_ITEMS=()

# ---------- 用法 ----------
usage() {
    cat <<EOF
用法: $0 [选项]

选项:
  -h, --help              显示此帮助信息
  --volume NAME           数据卷名称 (默认: snack-data)
  --keep-data             保留数据卷中的 db/certs/配置 (默认: true)
  --keep-data=true        显式保留数据卷
  --keep-data=false       不保留数据卷，提示是否删除
  --no-keep-data          不保留数据卷（等价 --keep-data=false）
  --yes                   跳过所有交互确认，直接执行

示例:
  $0                      # 交互式卸载，默认保留数据卷、镜像、compose.yaml
  $0 --yes                # 无交互卸载，保留所有数据
  $0 --keep-data=false    # 提示删除数据卷
  $0 --yes --keep-data=false  # 无交互删除数据卷
EOF
    exit 0
}

# ---------- 参数解析 ----------
while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)
            usage
            ;;
        --volume)
            VOLUME_NAME="$2"
            shift 2
            ;;
        --volume=*)
            VOLUME_NAME="${1#*=}"
            shift
            ;;
        --keep-data)
            KEEP_DATA=true
            shift
            ;;
        --keep-data=true)
            KEEP_DATA=true
            shift
            ;;
        --keep-data=false|--no-keep-data)
            KEEP_DATA=false
            shift
            ;;
        --yes)
            YES=true
            shift
            ;;
        *)
            log_error "未知参数: $1"
            usage
            ;;
    esac
done

# ---------- 环境检查 ----------
if ! command -v docker &>/dev/null; then
    log_error "docker 未安装，无法卸载"
    exit 1
fi

if docker compose version &>/dev/null 2>&1; then
    COMPOSE_CMD="docker compose"
elif command -v docker-compose &>/dev/null; then
    COMPOSE_CMD="docker-compose"
else
    log_error "docker compose 未安装，无法卸载"
    exit 1
fi

# 脚本所在目录即项目根目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_PATH="$SCRIPT_DIR/$COMPOSE_FILE"

echo ""
log_info "================ Docker 卸载 ================"
log_info "容器名:    $CONTAINER_NAME"
log_info "数据卷:    $VOLUME_NAME"
log_info "镜像名:    $IMAGE_NAME"
log_info "保留数据:  $KEEP_DATA"
log_info "自动确认:  $YES"
echo ""

# ---------- 检查容器是否存在 ----------
CONTAINER_EXISTS=false
if docker ps -a --format '{{.Names}}' | grep -qx "$CONTAINER_NAME"; then
    CONTAINER_EXISTS=true
fi

if [[ "$CONTAINER_EXISTS" == "false" ]]; then
    log_warn "容器 '$CONTAINER_NAME' 不存在，无需卸载"
    exit 0
fi

# ---------- 检查容器运行状态 ----------
CONTAINER_RUNNING=false
if docker ps --format '{{.Names}}' | grep -qx "$CONTAINER_NAME"; then
    CONTAINER_RUNNING=true
fi

# ---------- 交互确认（默认 N） ----------
if [[ "$YES" == "false" ]]; then
    log_prompt "确定要卸载容器 '$CONTAINER_NAME' 吗？(y/N) "
    read -r confirm </dev/tty
    if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
        log_info "已取消卸载"
        exit 0
    fi
fi

# ---------- 停止并移除容器与网络 ----------
if [[ "$CONTAINER_RUNNING" == "true" ]]; then
    log_info "容器运行中，执行 docker compose down..."
    if [[ -f "$COMPOSE_PATH" ]]; then
        (cd "$SCRIPT_DIR" && $COMPOSE_CMD down)
    else
        log_warn "compose.yaml 不存在，直接停止并移除容器"
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
    fi
    log_success "容器已停止并移除"
    DELETED_ITEMS+=("容器: $CONTAINER_NAME")
else
    log_info "容器未运行，移除容器..."
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    log_success "容器已移除"
    DELETED_ITEMS+=("容器: $CONTAINER_NAME")
fi

# ---------- 数据卷处理 ----------
if [[ "$KEEP_DATA" == "true" ]]; then
    log_info "保留数据卷: $VOLUME_NAME"
    KEPT_ITEMS+=("数据卷: $VOLUME_NAME")
else
    delete_volume=false
    if [[ "$YES" == "true" ]]; then
        delete_volume=true
    else
        log_prompt "是否删除数据卷 '$VOLUME_NAME'？(y/N) "
        read -r vol_input </dev/tty
        if [[ "$vol_input" =~ ^[Yy]$ ]]; then
            delete_volume=true
        fi
    fi

    if [[ "$delete_volume" == "true" ]]; then
        if docker volume ls --format '{{.Name}}' | grep -qx "$VOLUME_NAME"; then
            docker volume rm "$VOLUME_NAME"
            log_success "数据卷已删除: $VOLUME_NAME"
            DELETED_ITEMS+=("数据卷: $VOLUME_NAME")
        else
            log_warn "数据卷 '$VOLUME_NAME' 不存在，跳过"
            KEPT_ITEMS+=("数据卷: $VOLUME_NAME (不存在)")
        fi
    else
        log_info "保留数据卷: $VOLUME_NAME"
        KEPT_ITEMS+=("数据卷: $VOLUME_NAME")
    fi
fi

# ---------- 镜像处理 ----------
delete_image=false
if [[ "$YES" == "true" ]]; then
    delete_image=true
else
    log_prompt "是否删除镜像 '$IMAGE_NAME'？(y/N) "
    read -r img_input </dev/tty
    if [[ "$img_input" =~ ^[Yy]$ ]]; then
        delete_image=true
    fi
fi

if [[ "$delete_image" == "true" ]]; then
    if docker images --format '{{.Repository}}:{{.Tag}}' | grep -q "^${IMAGE_NAME}"; then
        docker rmi "$IMAGE_NAME" 2>/dev/null || true
        log_success "镜像已删除: $IMAGE_NAME"
        DELETED_ITEMS+=("镜像: $IMAGE_NAME")
    else
        log_warn "镜像 '$IMAGE_NAME' 不存在，跳过"
        KEPT_ITEMS+=("镜像: $IMAGE_NAME (不存在)")
    fi
else
    log_info "保留镜像: $IMAGE_NAME"
    KEPT_ITEMS+=("镜像: $IMAGE_NAME")
fi

# ---------- compose.yaml 处理 ----------
delete_compose=false
if [[ "$YES" == "true" ]]; then
    delete_compose=true
else
    log_prompt "是否删除项目根目录的 compose.yaml？(y/N) "
    read -r comp_input </dev/tty
    if [[ "$comp_input" =~ ^[Yy]$ ]]; then
        delete_compose=true
    fi
fi

if [[ "$delete_compose" == "true" ]]; then
    if [[ -f "$COMPOSE_PATH" ]]; then
        rm -f "$COMPOSE_PATH"
        log_success "compose.yaml 已删除"
        DELETED_ITEMS+=("文件: compose.yaml")
    else
        log_warn "compose.yaml 不存在，跳过"
        KEPT_ITEMS+=("文件: compose.yaml (不存在)")
    fi
else
    log_info "保留 compose.yaml（作为文档）"
    KEPT_ITEMS+=("文件: compose.yaml")
fi

# ---------- 清理完成摘要 ----------
echo ""
log_info "================ 清理完成摘要 ================"
if [[ ${#DELETED_ITEMS[@]} -gt 0 ]]; then
    echo -e "${RED}已删除:${NC}"
    for item in "${DELETED_ITEMS[@]}"; do
        echo "  - $item"
    done
else
    echo -e "${RED}已删除:${NC} (无)"
fi
if [[ ${#KEPT_ITEMS[@]} -gt 0 ]]; then
    echo -e "${GREEN}已保留:${NC}"
    for item in "${KEPT_ITEMS[@]}"; do
        echo "  - $item"
    done
else
    echo -e "${GREEN}已保留:${NC} (无)"
fi
echo ""
log_success "卸载完成"
