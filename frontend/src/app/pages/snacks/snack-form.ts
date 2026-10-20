import { SnackInput, SnackLifecycle } from '../../models';
import { currentLocalDate } from '../../snack-status';

/** 零食编辑弹窗的表单模型（日期字段用字符串，空串表示“未填写”）。 */
export interface SnackFormModel {
  name: string;
  quantity: string;
  location: string;
  purchase_date: string;
  production_date: string;
  shelf_life_days: string;
  expiry_date: string;
  notes: string;
  status: SnackLifecycle;
  opened_at: string;
  finished_at: string;
}

/**
 * 生命周期状态与打开/吃完日期的一致性规则（契约 §3：UI 不应展示矛盾数据）：
 * - `in_stock`（未打开）：打开日期与吃完日期都必须为空；
 * - `opened`（已打开）：保留打开日期，吃完日期必须为空；
 * - `finished`（已吃完）：保留打开日期，吃完日期必填（留空则填当前日期）。
 */
export function lifecycleTimes(
  status: SnackLifecycle,
  openedAt: string | null,
  finishedAt: string | null,
): { opened_at: string | null; finished_at: string | null } {
  if (status === 'finished') {
    return { opened_at: openedAt || null, finished_at: finishedAt || currentLocalDate() };
  }
  if (status === 'opened') {
    return { opened_at: openedAt || null, finished_at: null };
  }
  return { opened_at: null, finished_at: null };
}

/** 生产日期 / 保质期 / 过期日期中，用户本次修改的字段。 */
export type LifecycleDateField = 'production_date' | 'shelf_life_days' | 'expiry_date';

/**
 * 生产日期、保质期（天）、过期日期：填任意两项自动计算第三项。
 *
 * 以用户本次修改的字段为基准，改写另外一项（若第三项原已存在则覆盖），
 * 因此三项始终自洽；日期非法、保质期为负或过期早于生产时返回空对象，
 * 由表单校验给出错误提示。
 */
export function deriveDates(
  form: SnackFormModel,
  changed: LifecycleDateField,
): Partial<SnackFormModel> {
  const production = form.production_date.trim();
  const expiry = form.expiry_date.trim();
  const shelfText = form.shelf_life_days.trim();
  const shelf = shelfText === '' ? null : Number(shelfText);
  if (shelf !== null && (!Number.isInteger(shelf) || shelf < 0)) {
    return {};
  }

  if (changed === 'production_date') {
    if (!production) {
      return {};
    }
    if (shelf !== null) {
      const computed = addDays(production, shelf);
      return computed && computed !== expiry ? { expiry_date: computed } : {};
    }
    if (expiry) {
      const days = daysBetween(production, expiry);
      return days !== null && days >= 0
        ? { shelf_life_days: String(days) }
        : {};
    }
    return {};
  }

  if (changed === 'shelf_life_days') {
    if (shelf === null) {
      return {};
    }
    if (production) {
      const computed = addDays(production, shelf);
      return computed && computed !== expiry ? { expiry_date: computed } : {};
    }
    if (expiry) {
      const computed = addDays(expiry, -shelf);
      return computed ? { production_date: computed } : {};
    }
    return {};
  }

  // changed === 'expiry_date'
  if (!expiry) {
    return {};
  }
  if (production) {
    const days = daysBetween(production, expiry);
    return days !== null && days >= 0
      ? { shelf_life_days: String(days) }
      : {};
  }
  if (shelf !== null) {
    const computed = addDays(expiry, -shelf);
    return computed ? { production_date: computed } : {};
  }
  return {};
}

/** 解析 YYYY-MM-DD 为 UTC 毫秒；非法日期（如 2026-02-30）返回 null。 */
function parseDate(value: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) {
    return null;
  }
  const [year, month, day] = value.split('-').map(Number);
  const ms = Date.UTC(year, month - 1, day);
  const date = new Date(ms);
  if (
    date.getUTCFullYear() !== year ||
    date.getUTCMonth() !== month - 1 ||
    date.getUTCDate() !== day
  ) {
    return null;
  }
  return ms;
}

/** 日期加减天数；结果格式 YYYY-MM-DD，非法输入返回 null。 */
export function addDays(date: string, days: number): string | null {
  const ms = parseDate(date);
  if (ms === null) {
    return null;
  }
  return new Date(ms + days * 86_400_000).toISOString().slice(0, 10);
}

/** 两个日期相差的天数（to - from）；非法输入返回 null。 */
export function daysBetween(from: string, to: string): number | null {
  const start = parseDate(from);
  const end = parseDate(to);
  if (start === null || end === null) {
    return null;
  }
  return Math.round((end - start) / 86_400_000);
}

/** 表单 → POST/PUT 提交体：裁剪文本字段并按 `lifecycleTimes` 归一化状态/日期。 */
export function formToSnackInput(form: SnackFormModel): SnackInput {
  return {
    name: form.name.trim(),
    quantity: Number(form.quantity),
    location: form.location.trim(),
    purchase_date: form.purchase_date || null,
    production_date: form.production_date || null,
    shelf_life_days:
      form.shelf_life_days.trim() === '' ? null : Number(form.shelf_life_days),
    expiry_date: form.expiry_date || null,
    notes: form.notes.trim() || null,
    status: form.status,
    ...lifecycleTimes(form.status, form.opened_at, form.finished_at),
  };
}
