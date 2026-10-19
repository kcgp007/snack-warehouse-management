import { Snack, SnackLifecycle } from './models';

export type SnackBadgeClass = 'expired' | 'soon' | 'ok' | 'none';

export interface SnackBadge {
  cls: SnackBadgeClass;
  label: string;
  daysLeft: number | null;
}

export interface LifecycleBadge {
  cls: SnackLifecycle;
  label: string;
}

/** 基于本机本地日期计算距离目标日期（YYYY-MM-DD）的天数。 */
export function daysUntil(date: string): number {
  const [y, m, d] = date.split('-').map(Number);
  const target = Date.UTC(y, m - 1, d);
  const now = new Date();
  const today = Date.UTC(now.getFullYear(), now.getMonth(), now.getDate());
  return Math.round((target - today) / 86_400_000);
}

/** 过期状态徽章（基于 expiry_date）：已过期=红、≤7天=橙、正常=绿、无过期日期=灰。 */
export function snackBadge(expiryDate: string | null): SnackBadge {
  if (!expiryDate) {
    return { cls: 'none', label: '无过期日期', daysLeft: null };
  }
  const daysLeft = daysUntil(expiryDate);
  if (daysLeft < 0) {
    return { cls: 'expired', label: '已过期', daysLeft };
  }
  if (daysLeft <= 7) {
    return { cls: 'soon', label: `剩 ${daysLeft} 天`, daysLeft };
  }
  return { cls: 'ok', label: '正常', daysLeft };
}

export function snackBadgeOf(snack: Snack): SnackBadge {
  return snackBadge(snack.expiry_date);
}

/** 生命周期状态徽章：in_stock=蓝"未打开"、opened=紫"已打开"、finished=灰"已吃完"。 */
export function lifecycleBadge(status: SnackLifecycle): LifecycleBadge {
  switch (status) {
    case 'opened':
      return { cls: 'opened', label: '已打开' };
    case 'finished':
      return { cls: 'finished', label: '已吃完' };
    default:
      return { cls: 'in_stock', label: '未打开' };
  }
}

/** 当前本地日期，格式 YYYY-MM-DD（与契约日期格式一致）。 */
export function currentLocalDate(): string {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}
