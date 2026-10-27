# 零食仓库管理平台

管理家庭/仓库零食库存的单体 Web 应用：记录每件零食的**存放位置、生产日期、保质期、购买日期、过期日期**，在临期时给出**过期提醒**，并通过登录与管理员账户管理保护数据。

## 功能特性

- 零食库存管理：名称、数量、存放位置、购买日期、生产日期、保质期（支持天/月/年）、过期日期、备注；生产日期/保质期/过期日期任意两项自动计算第三项（三项矛盾会被拒绝）；支持搜索、按位置过滤；位置输入支持已有位置自动补全。
- 双重状态维度：过期状态（基于过期日期）与生命周期状态（由打开/吃完日期自动推断），状态不接受手动选择，并校验“打开日期不能晚于吃完日期”；支持一键“标记打开/标记吃完”。
- 过期提醒：`/reminders` 页面按分级（已过期/≤3 天/≤7 天）展示即将过期的零食与剩余天数；已吃完的零食不再出现在提醒中。
- 登录认证：Bearer token 认证，401 自动跳转登录页，支持修改密码；启动时清理过期会话。
- admin 账户管理：管理员可查看/创建/删除普通账户（禁止删除自己），非管理员访问返回 403；首次启动自动初始化 admin 账户。
- 单进程部署：无 Nginx / API 网关 / 独立认证服务等中间件，Angular 构建产物由 Rust 后端直接托管（含 SPA fallback）。

## 技术栈

| 层 | 技术 |
|---|---|
| 后端 | Rust + axum，SQLite，后端 API ( /api/* ) |
| 前端 | Angular（standalone components + Router + guards），SPA 托管 |

## 目录结构

snack-warehouse-management/
├── backend/  # Rust 后端源码
│   ├── src/   # axum 应用（路由、认证、数据库、提示、reminders 等）
│   └── tests/ # 集成测试
├── frontend/ # Angular 前端源码
│   └── src/app/ # 页面与组件（登录、snacks、reminders、users、change-password 等）
└── CONTRACT.md # 前后端集成契约，API 合同、校验规则、排序、测试清单（唯一事实来源）

完整契约见 [`CONTRACT.md`](CONTRACT.md)。https://github.com/kcgp007/snack-warehouse-management
