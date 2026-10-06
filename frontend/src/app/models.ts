/** 与 CONTRACT.md §3 对应的数据结构（前后端唯一事实来源）。 */

export interface User {
  id: number;
  username: string;
  is_admin: boolean;
  /** 仅 GET /api/users 等接口返回 */
  created_at?: string;
}

export interface LoginResponse {
  token: string;
  user: User;
}

/** 生命周期状态：in_stock 未打开 / opened 已打开 / finished 已吃完 */
export type SnackLifecycle = 'in_stock' | 'opened' | 'finished';

/** 过期状态筛选（请求参数 expiry_status） */
export type ExpiryStatusFilter = 'all' | 'ok' | 'expiring' | 'expired';

/** 生命周期状态筛选（请求参数 status） */
export type LifecycleStatusFilter = 'all' | SnackLifecycle;

/** POST/PUT /api/snacks 的请求体；日期 YYYY-MM-DD，时间 YYYY-MM-DDTHH:MM，均可为 null */
export interface SnackInput {
  name: string;
  quantity: number;
  location: string;
  purchase_date: string | null;
  expiry_date: string | null;
  notes: string | null;
  status: SnackLifecycle;
  opened_at: string | null;
  finished_at: string | null;
}

export interface Snack extends SnackInput {
  id: number;
  created_at: string;
  updated_at: string;
}

/** 过期提醒分级（注意与 Snack.status 生命周期状态区分） */
export type ReminderLevel = 'expired' | 'critical' | 'warning';

/** GET /api/reminders 元素：Snack（含新字段）+ days_left + level */
export interface Reminder extends Snack {
  days_left: number;
  level: ReminderLevel;
}
