#!/usr/bin/env bash
set -euo pipefail

# 一键安装脚本 — 系统级守护进程（systemd）
# 安装目录: /opt/snack-warehouse
# 运行用户: snack-warehouse (uid<1000 系统账户)
# 数据目录: /var/lib/snack-warehouse (data/ + snack-warehouse.toml)
# 配置: /etc/snack-warehouse.toml 或运行目录同名

INSTALL_DIR="/opt/snack-warehouse"
RUN_USER="snack-warehouse"
DATA_DIR="/var/lib/snack-warehouse"
CONFIG_FILE="/etc/snack-warehouse.toml"

usage() {
    echo "用法: $0 [--no-build] [--profile debug|release]"
    echo "  --no-build  跳过编译，直接安装已存在的 dist/"
    echo "  --profile   编译配置: debug (默认) 或 release"
    exit 0
}

# 参数解析
PROFILE="debug"
BUILD=1
while [[ $# -gt 0 ]]; do
    case "$1" in
        --help|-h) usage ;;
        --no-build) BUILD=0; shift ;;
        --profile) PROFILE="$2"; shift 2 ;;
        *) echo "未知参数: $1"; exit 1 ;;
    esac
done

echo "[install] 开始系统级安装到 ${INSTALL_DIR}"

# 创建运行用户（系统账户，uid<1000，不创建家目录）
if ! id -u "$RUN_USER" &>/dev/null; then
    useradd --system --uid 999 --shell /usr/sbin/nologin --no-create-home --home-dir /nonexistent "$RUN_USER"
    echo "[install] 创建系统用户: $RUN_USER (uid=999)"
else
    echo "[install] 用户已存在: $RUN_USER"
fi

# 创建数据目录
dirs=("$DATA_DIR" "$INSTALL_DIR" "$INSTALL_DIR/static")
for d in "${dirs[@]}"; do
    mkdir -p "$d"
done

# 数据目录权限 750，运行用户可访问
chown -R root:"$RUN_USER" "$DATA_DIR"
chmod 750 "$DATA_DIR"

# 构建（若需要）
if [[ "$BUILD" == "1" ]]; then
    echo "[install] 编译中..."
    ./build.sh 2>/dev/null || cargo build --${PROFILE} --bin snack-warehouse-backend
    cp -r dist/static "$INSTALL_DIR/static" || true
    cp dist/snack-warehouse-backend "$INSTALL_DIR/snack-warehouse-backend"
    echo "[install] 二进制已安装: $INSTALL_DIR/snack-warehouse-backend"
fi

# 安装二进制（若 --no-build 且已存在则跳过）
if [[ ! -f "$INSTALL_DIR/snack-warehouse-backend" ]]; then
    echo "[ERROR] 二进制缺失，且未执行构建。"
    exit 1
fi
chmod 755 "$INSTALL_DIR/snack-warehouse-backend"

# 配置文件生成（TOML，权限 640 root:snack-warehouse）
cat > "$CONFIG_FILE" << 'TOMLEOF'
host = "0.0.0.0"
port = 3000
db_path = "/var/lib/snack-warehouse/data/app.db"
static_dir = "/opt/snack-warehouse/static"
admin_username = "admin"
admin_password = "admin123"
TOMLEOF

cp "$CONFIG_FILE" "$DATA_DIR/snack-warehouse.toml" || true
chmod 640 "$CONFIG_FILE"
chown root:"$RUN_USER" "$CONFIG_FILE"
ls -la "$CONFIG_FILE"

echo "[install] 配置已写入: $CONFIG_FILE"
echo "[install] 请立即登录 修改默认密码 admin/admin123"

echo "[install] 安装完成。"
echo "[install] 启动方式: sudo systemctl start snack-warehouse"
echo "[install] 数据目录: $DATA_DIR"
echo "[install] 安装目录: $INSTALL_DIR"
