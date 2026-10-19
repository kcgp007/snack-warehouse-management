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
