import { Reminder } from './models';
import { groupRemindersByLevel } from './reminder-group';

function reminder(id: number, level: Reminder['level']): Reminder {
  return {
    id,
    name: `snack-${id}`,
    quantity: 1,
    location: '客厅/零食柜',
    purchase_date: null,
    production_date: null,
    shelf_life_days: null,
    expiry_date: '2026-01-01',
    notes: null,
    status: 'in_stock',
    opened_at: null,
    finished_at: null,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    days_left: -1,
    level,
  };
}

describe('groupRemindersByLevel', () => {
  it('groups by level in order expired → critical → warning', () => {
    const groups = groupRemindersByLevel([
      reminder(1, 'warning'),
      reminder(2, 'expired'),
      reminder(3, 'critical'),
      reminder(4, 'expired'),
    ]);
    expect(groups.map((group) => group.level)).toEqual(['expired', 'critical', 'warning']);
    expect(groups[0].items.map((item) => item.id)).toEqual([2, 4]);
    expect(groups[1].items.map((item) => item.id)).toEqual([3]);
    expect(groups[2].items.map((item) => item.id)).toEqual([1]);
  });

  it('omits empty groups', () => {
    const groups = groupRemindersByLevel([reminder(1, 'critical')]);
    expect(groups).toHaveLength(1);
    expect(groups[0].level).toBe('critical');
    expect(groupRemindersByLevel([])).toEqual([]);
  });
});
