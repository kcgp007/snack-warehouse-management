#!/usr/bin/env bash
#
# docker-update.yml — 滚动更新单体 Docker 镜像 snack-warehouse-management-app
#
# 用途：
#   将 compose.yaml 中的 app 服务镜像切换为新版本，滚动替换容器 snack-warehouse，
#   并在更新后容器不健康时自动回滚到之前的容器。
#
# 参数：
#   --image             新镜像名（默认 snack-warehouse-management-app:latest）
#   --pull-new-image    是否先拉取/构建新镜像（布尔，默认 false；由外部调用者决定）
#   --backup            更新前是否备份 snack-data 卷（布尔，默认 true）
#   -h / --help         显示帮助
#
set -euo pipefail

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 颜色日志
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info()  { printf "${BLUE}[INFO]${NC} %s\n" "$*"; }
log_ok()    { printf "${GREEN}[OK]${NC} %s\n" "$*"; }
log_warn()  { printf "${YELLOW}[WARN]${NC} %s\n" "$*"; }
log_err()   { printf "${RED}[ERROR]${NC} %s\n" "$*" >&2; }

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 默认值
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
IMAGE_DEFAULT="snack-warehouse-management-app:latest"
CONTAINER_NAME="snack-warehouse"
COMPOSE_FILE="compose.yaml"
VOLUME_NAME="snack-data"
BACKUP_PREFIX="backup"

IMAGE="${IMAGE_DEFAULT}"
PULL_NEW_IMAGE="false"
BACKUP="true"
OLD_IMAGE=""

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 参数解析
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
usage() {
    cat <<EOF
用法: $0 [选项]

选项:
  --image <镜像名>        新镜像名（默认: ${IMAGE_DEFAULT}）
  --pull-new-image        是否先 docker pull / docker build 更新镜像（布尔，默认: false）
  --backup                更新前是否备份 ${VOLUME_NAME} 数据卷（布尔，默认: true）
  -h, --help              显示此帮助信息

示例:
  $0 --image snack-warehouse-management-app:v2.1.0 --pull-new-image --backup
  $0 --image my-registry/snack-warehouse-management-app:latest --pull-new-image=false
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --image)
            [[ $# -ge 2 ]] || { log_err "--image 需要一个值"; exit 1; }
            IMAGE="$2"; shift 2 ;;
        --pull-new-image)
            [[ $# -ge 2 ]] || { log_err "--pull-new-image 需要一个布尔值"; exit 1; }
            PULL_NEW_IMAGE="$2"; shift 2 ;;
        --backup)
            [[ $# -ge 2 ]] || { log_err "--backup 需要一个布尔值"; exit 1; }
            BACKUP="$2"; shift 2 ;;
        -h|--help)
            usage ;;
        *)
            log_err "未知参数: $1"; exit 1 ;;
    esac
done

# 归一化布尔值（接受 true/false/1/0/yes/no）
to_bool() {
    local v="${1,,}" # lowercase
    case "$v" in
        true|1|yes|y) echo "true" ;;
        false|0|no|n) echo "false" ;;
        *) log_err "无效布尔值: $1（应为 true/false）"; exit 1 ;;
    esac
}
PULL_NEW_IMAGE="$(to_bool "$PULL_NEW_IMAGE")"
BACKUP="$(to_bool "$BACKUP")"

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 前置检查：docker / compose 可用
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
check_tools() {
    if ! command -v docker >/dev/null 2>&1; then
        log_err "未找到 docker 命令，请先安装 Docker。"
        exit 1
    fi

    local compose_cmd=""
    if docker compose version >/dev/null 2>&1; then
        compose_cmd="docker compose"
    elif command -v docker-compose >/dev/null 2>&1; then
        compose_cmd="docker-compose"
    else
        log_err "未找到 docker compose / docker-compose，请先安装 Docker Compose。"
        exit 1
    fi
    echo "$compose_cmd"
}

COMPOSE_CMD="$(check_tools)"
log_ok "检测到 compose 命令: ${COMPOSE_CMD}"

if [[ ! -f "$COMPOSE_FILE" ]]; then
    log_err "未找到 ${COMPOSE_FILE}，请在项目根目录下运行本脚本。"
    exit 1
fi

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 检查容器 snack-warehouse 是否存在并运行
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
check_container() {
    log_info "检查容器 ${CONTAINER_NAME} 状态..."
    if ! docker ps -a --format '{{.Names}}' | grep -qx "${CONTAINER_NAME}"; then
        log_err "容器 ${CONTAINER_NAME} 不存在，请先运行 docker-install.sh 部署。"
        exit 1
    fi

    local status
    status="$(docker inspect -f '{{.State.Status}}' "${CONTAINER_NAME}" 2>/dev/null || echo "unknown")"
    if [[ "$status" != "running" ]]; then
        log_err "容器 ${CONTAINER_NAME} 当前状态为 ${status}，非 running，请先运行 docker-install.sh 启动。"
        exit 1
    fi
    log_ok "容器 ${CONTAINER_NAME} 正在运行。"
}

check_container

# 记录旧容器 ID（用于回滚与输出）
OLD_CONTAINER_ID="$(docker inspect -f '{{.Id}}' "${CONTAINER_NAME}")"
log_info "旧容器 ID: ${OLD_CONTAINER_ID}"

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 备份 snack-data 卷
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
do_backup() {
    local ts
    ts="$(date +%Y%m%d-%H%M%S)"
    local backup_file="${BACKUP_PREFIX}-${ts}.tar.gz"

    log_info "开始备份卷 ${VOLUME_NAME} -> ${backup_file}"
    # 以临时容器挂载卷并打包当前目录下的文件
    docker run --rm \
        -v "${VOLUME_NAME}:/source:ro" \
        -v "$(pwd):/dest" \
        alpine tar czf "/dest/${backup_file}" -C /source . \
        >/dev/null 2>&1 || {
        log_err "备份卷 ${VOLUME_NAME} 失败。"
        exit 1
    }
    log_ok "备份完成: ${backup_file}"
    echo "$backup_file"
}

BACKUP_FILE=""
if [[ "$BACKUP" == "true" ]]; then
    BACKUP_FILE="$(do_backup)"
else
    log_info "已跳过备份（--backup=false）。"
fi

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 可选：拉取/构建新镜像
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
update_image() {
    if [[ "$PULL_NEW_IMAGE" == "true" ]]; then
        log_info "检测到 --pull-new-image=true，准备更新镜像 ${IMAGE}"

        # 判断 compose.yaml 中 app 服务是否使用 build: 或 image:
        if grep -qE '^[[:space:]]*build:' "$COMPOSE_FILE"; then
            log_info "compose.yaml 使用 build: .，执行 docker build ${IMAGE}"
            docker build -t "${IMAGE}" . >/dev/null
        else
            log_info "执行 docker pull ${IMAGE}"
            docker pull "${IMAGE}" >/dev/null
        fi
        log_ok "镜像 ${IMAGE} 已就绪。"
    else
        log_info "--pull-new-image=false，跳过镜像拉取/构建（由外部调用者负责镜像就绪）。"
    fi
}

update_image

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 修改 compose.yaml 镜像标签
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
set_compose_image() {
    local compose_img=""
    compose_img="$(grep -E '^[[:space:]]*image:' "$COMPOSE_FILE" | head -n1 | sed -E 's/.*image:[[:space:]]*//' | tr -d '"' || true)"

    if [[ -n "$compose_img" ]]; then
        OLD_IMAGE="$compose_img"
        log_info "compose.yaml 中已存在 image: ${compose_img}，替换为 ${IMAGE}"
        # 替换 image: 行（可移植写法，替换整行）
        awk -v img="$IMAGE" '!done && /^[[:space:]]*image:/ {print "    image: " img; done=1; next} {print}' "$COMPOSE_FILE" > "$COMPOSE_FILE.tmp" && mv "$COMPOSE_FILE.tmp" "$COMPOSE_FILE"
    else
        log_info "compose.yaml 中未发现 image:，在 app 服务下添加 image: ${IMAGE}"
        # 在 `services:` 下的 `app:` 区块追加 image 行（位于 build: 之后）
        # 定位 `app:` 行后，在其下一行插入 image:
        local app_line
        app_line="$(grep -nE '^[[:space:]]*app:' "$COMPOSE_FILE" | head -n1 | cut -d: -f1)"
        if [[ -z "$app_line" ]]; then
            log_err "compose.yaml 中未找到 app 服务，无法设置镜像。"
            exit 1
        fi
        awk -v img="$IMAGE" -v n="$app_line" 'NR==n {print; print "    image: " img; next} {print}' "$COMPOSE_FILE" > "$COMPOSE_FILE.tmp" && mv "$COMPOSE_FILE.tmp" "$COMPOSE_FILE"
    fi
    log_ok "compose.yaml 镜像已设为 ${IMAGE}"
}

set_compose_image

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 滚动替换：up 新容器，stop/rm 旧容器
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
do_rollout() {
    log_info "执行 docker compose up -d --pull=never 滚动替换容器..."
    if [[ "$PULL_NEW_IMAGE" == "true" ]] && grep -qE '^[[:space:]]*build:' "$COMPOSE_FILE"; then
        # 本地镜像场景：先 build 再 up（不 pull）
        docker compose -f "$COMPOSE_FILE" up -d --build
    else
        docker compose -f "$COMPOSE_FILE" up -d --pull=never
    fi
    log_ok "docker compose up 完成。"
}

do_rollout

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 等待健康检查
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
wait_healthy() {
    local max_wait="${1:-120}"
    local elapsed=0
    local interval=5

    log_info "等待容器 ${CONTAINER_NAME} 健康（最多 ${max_wait}s）..."
    while [[ $elapsed -lt $max_wait ]]; do
        local health
        health="$(docker inspect -f '{{.State.Health.Status}}' "${CONTAINER_NAME}" 2>/dev/null || echo "none")"
        case "$health" in
            healthy)
                log_ok "容器 ${CONTAINER_NAME} 已健康。"
                return 0
                ;;
            unhealthy)
                log_err "容器 ${CONTAINER_NAME} 变为 unhealthy。"
                return 1
                ;;
            *)
                log_info "当前健康状态: ${health}（已等待 ${elapsed}s）"
                sleep "$interval"
                elapsed=$((elapsed + interval))
                ;;
        esac
    done

    log_err "等待容器 ${CONTAINER_NAME} 健康超时（${max_wait}s）。"
    return 1
}

HEALTHY="false"
if wait_healthy 120; then
    HEALTHY="true"
else
    log_err "更新后容器 ${CONTAINER_NAME} 不健康，准备回滚。"
fi

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 回滚：若新容器不健康，尝试恢复旧容器
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
rollback() {
    log_warn "尝试回滚到之前的容器..."

    # 停止并移除当前（不健康）的容器
    docker compose -f "$COMPOSE_FILE" stop >/dev/null 2>&1 || true
    docker compose -f "$COMPOSE_FILE" rm -f >/dev/null 2>&1 || true

    # 检查旧镜像是否存在本地
    if ! docker images -q "${OLD_IMAGE}" >/dev/null 2>&1; then
        log_err "本地不存在镜像 ${OLD_IMAGE}，无法回滚。"
        return 1
    fi

    # 将 compose.yaml 的 image: 恢复为 OLD_IMAGE
    if [[ -n "$OLD_IMAGE" ]]; then
        awk -v img="$OLD_IMAGE" '!done && /^[[:space:]]*image:/ {print "    image: " img; done=1; next} {print}' "$COMPOSE_FILE" > "$COMPOSE_FILE.tmp" && mv "$COMPOSE_FILE.tmp" "$COMPOSE_FILE"
        log_info "compose.yaml 镜像已恢复为 ${OLD_IMAGE}"
    fi

    # 重建容器
    docker compose -f "$COMPOSE_FILE" up -d
    log_ok "已回滚并重建容器 ${CONTAINER_NAME}。"
}

if [[ "$HEALTHY" != "true" ]]; then
    if rollback; then
        log_err "更新失败，已回滚到之前的容器。旧容器 ID: ${OLD_CONTAINER_ID}"
    else
        log_err "更新失败且回滚也未成功。旧容器 ID: ${OLD_CONTAINER_ID}"
    fi
    exit 1
fi

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 清理旧容器（已被 up -d 替换），输出完成信息
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 移除已停止的旧容器（若仍残留）
docker compose -f "$COMPOSE_FILE" rm -f >/dev/null 2>&1 || true

log_ok "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
log_ok "更新完成"
log_ok "  新镜像:      ${IMAGE}"
log_ok "  容器名:      ${CONTAINER_NAME}"
log_ok "  旧容器 ID:   ${OLD_CONTAINER_ID}"
log_ok "  备份文件:    ${BACKUP_FILE:-<无>}"
log_ok "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

exit 0
