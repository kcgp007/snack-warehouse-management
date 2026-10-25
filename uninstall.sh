#!/usr/bin/env bash
set -euo pipefail

# 一键卸载脚本 — 交互式确认，默认保留数据与配置
# 普通账户永不删除系统用户（uid<1000 才询问删除）
# 无 --purge/--yes 参数，改为交互式 Y/N（默认 N 保留）

INSTALL_DIR="/opt/snack-warehouse"
RUN_USER="snack-warehouse"
DATA_DIR="/var/lib/snack-warehouse"
CONFIG_FILE="/etc/snack-warehouse.toml"
SERVICE_NAME="snack-warehouse"

usage() {
    echo "用法: $0 [--yes] [--purge]"
    echo "  --yes    跳过交互式确认（默认 N）"
    echo "  --purge  额外删除运行用户/组（仅 uid<1000 系统账户才询问，普通账户永不删除）"
    exit 0
}

YES=false
PURGE=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --help|-h) usage ;;
        --yes) YES=true; shift ;;
        --purge) PURGE=true; shift ;;
        *) echo "未知参数: $1"; exit 1 ;;
    esac
done

confirm() {
    local msg="$1"
    local default="$2"
    if [[ "$YES" == "true" ]]; then
        echo "[uninstall] $msg -> 自动确认（--yes 设定）"
        return 0
    fi
    read -r -p "$msg [Y/N, 默认 $default] " ans < /dev/tty || true
    case "${ans:-$default}" in
        [Yy]* ) return 0 ;;
        * ) return 1 ;;
    esac
}

echo "[uninstall] 开始卸载 ${SERVICE_NAME}"

# 1. 停止并禁用 systemd 服务（如果存在）
if command -v systemctl &>/dev/null && systemctl list-unit-files | grep -q "${SERVICE_NAME}"; then
    echo "[uninstall] 停止服务 ${SERVICE_NAME}"
    systemctl stop "${SERVICE_NAME}" 2>/dev/null || true
    systemctl disable "${SERVICE_NAME}" 2>/dev/null || true
    echo "[uninstall] 服务已停止/禁用"
else
    echo "[uninstall] 未检测到 systemd 服务，跳过"
fi

# 2. 删除程序/静态资源（总是删除），保留 data/ 与 TOML（由交互决定）
echo "[uninstall] 删除程序目录: ${INSTALL_DIR}"
if [[ -d "$INSTALL_DIR" ]]; then
    # 只删除 program/unit/静态资源，保留 data/ 与配置（交互式决定是否删除）
    find "$INSTALL_DIR" -mindepth 1 -maxdepth 1 ! -name 'data' ! -name 'snack-warehouse.toml' -exec rm -rf -- {} + 2>/dev/null || true
    echo "[uninstall] 程序目录已清理（data/ 和 TOML 保留）"
fi

# 3. 数据与配置（交互式 Y/N，默认 N 保留）
if confirm "删除数据目录 ${DATA_DIR} 与配置 ${CONFIG_FILE}？" "N"; then
    rm -rf "$DATA_DIR" 2>/dev/null || true
    rm -f "$CONFIG_FILE" 2>/dev/null || true
    echo "[uninstall] 数据与配置已删除"
else
    echo "[uninstall] 数据与配置已保留"
fi

# 4. 删除系统安装目录（若已清空）
if [[ -d "$INSTALL_DIR" ]]; then
    rmdir "$INSTALL_DIR" 2>/dev/null || true
fi

# 5. 删除运行用户（仅 uid<1000 系统账户才询问，普通账户永不删除）
if [[ "$PURGE" == "true" ]]; then
    USER_UID=$(id -u "$RUN_USER" 2>/dev/null || echo 10000)
    if [[ "$USER_UID" -lt 1000 ]]; then
        if confirm "删除系统用户 ${RUN_USER} (uid=${USER_UID})？" "N"; then
            userdel "$RUN_USER" 2>/dev/null || true
            groupdel "$RUN_USER" 2>/dev/null || true
            echo "[uninstall] 系统用户已删除"
        fi
    else
        echo "[uninstall] 普通账户 (${RUN_USER} uid=${USER_UID}) 永不删除"
    fi
fi

echo "[uninstall] 卸载完成。"
echo "[uninstall] 提示: 如需完全清理（含数据），请手动删除 ${DATA_DIR} 与 ${CONFIG_FILE}"
