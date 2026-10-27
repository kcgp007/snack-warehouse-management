# API 契约（前后端唯一事实来源）

## 认证接口

| 方法/路径 | 请求体 | 成功响应 | 说明 |
|---|---|---|---|
| POST /api/auth/login | {"username","password"} | {"token":string,"user":{"id","username","is_admin":bool}} | 失败 401 |
| POST /api/auth/logout | — | 204 | 使 token 失效 |
| GET /api/auth/me | — | 同 login user 对象 | |
| POST /api/auth/change-password | {"old_password","new_password"} | 204 | 新密码 <6 字符 → 400 |

## 零食接口

| 方法/路径 | 说明 |
|---|---|
| GET /api/snacks | 查询参数：q（模糊）、location（精确）、expiry_status（all/ok/expiring/expired）、status（all/in_stock/opened/finished），默认 `created_at DESC, id DESC`，裸数组 |
| POST /api/snacks | 请求体见 SnackInput；201；校验失败 400 |
| PUT /api/snacks/{id} | 同创建校验；不存在 404 |
| DELETE /api/snacks/{id} | 204；不存在 404 |
| GET /api/locations | 去重字符串数组（字典序）；可选 q 子串过滤，缺省/空串返回全部 |

## SnackInput

{ "name": string(必填), "quantity": int(≥0), "location": string(必填),
  "purchase_date": "YYYY-MM-DD", "production_date": null,
  "shelf_life_days": int, "shelf_life_unit": "day/month/year",
  "expiry_date": null, "notes": null,
  "status": null, "opened_at": null, "finished_at": null }

校验：日期仅接受 YYYY-MM-DD（传 YYYY-MM-DDTHH:MM → 400）；三项日期互不自洽 → 400；opened_at > finished_at → 400；status 与日期不一致 → 400。

## 提醒接口

GET /api/reminders → Snack + days_left + level (expired/critical/warning)。排除 finished，按过期日期升序。

## 账户管理（仅 admin）

GET /api/users / POST /api/users / DELETE /api/users/{id} → 列表按 username ASC；删除自己 → 400；不存在 404。
