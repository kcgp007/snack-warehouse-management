import { Reminder, ReminderLevel } from './models';

export interface ReminderGroup {
  level: ReminderLevel;
  title: string;
  items: Reminder[];
}

const GROUP_ORDER: ReminderLevel[] = ['expired', 'critical', 'warning'];

const GROUP_TITLES: Record<ReminderLevel, string> = {
  expired: '已过期',
  critical: '紧急（3 天内到期）',
  warning: '预警（4–7 天内到期）',
};

/** 按提醒分级 level 分组（已过期 → 紧急 → 预警），空组不返回。 */
export function groupRemindersByLevel(list: Reminder[]): ReminderGroup[] {
  return GROUP_ORDER.map((level) => ({
    level,
    title: GROUP_TITLES[level],
    items: list.filter((item) => item.level === level),
  })).filter((group) => group.items.length > 0);
}
