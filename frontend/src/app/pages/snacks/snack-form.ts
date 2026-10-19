import { SnackInput, SnackLifecycle } from '../../models';
import { currentLocalDate } from '../../snack-status';

/** 零食编辑弹窗的表单模型（日期字段用字符串，空串表示“未填写”）。 */
export interface SnackFormModel {
  name: string;
  quantity: string;
  location: string;
  purchase_date: string;
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

/** 表单 → POST/PUT 提交体：裁剪文本字段并按 `lifecycleTimes` 归一化状态/日期。 */
export function formToSnackInput(form: SnackFormModel): SnackInput {
  return {
    name: form.name.trim(),
    quantity: Number(form.quantity),
    location: form.location.trim(),
    purchase_date: form.purchase_date || null,
    expiry_date: form.expiry_date || null,
    notes: form.notes.trim() || null,
    status: form.status,
    ...lifecycleTimes(form.status, form.opened_at, form.finished_at),
  };
}
