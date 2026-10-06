import { currentLocalMinute, daysUntil, lifecycleBadge, snackBadge } from './snack-status';

function formatDate(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${date.getFullYear()}-${month}-${day}`;
}

function shifted(days: number): string {
  const date = new Date();
  date.setDate(date.getDate() + days);
  return formatDate(date);
}

describe('snackBadge', () => {
  it('marks a missing expiry date as none (灰色)', () => {
    expect(snackBadge(null).cls).toBe('none');
    expect(snackBadge(null).label).toBe('无过期日期');
  });

  it('marks a past expiry date as expired (红色)', () => {
    const badge = snackBadge('2000-01-01');
    expect(badge.cls).toBe('expired');
    expect(badge.daysLeft).toBeLessThan(0);
  });

  it('marks expiry within 7 days as soon (橙色) and beyond as ok (绿色)', () => {
    expect(snackBadge(shifted(0)).cls).toBe('soon');
    expect(snackBadge(shifted(7)).cls).toBe('soon');
    expect(snackBadge(shifted(8)).cls).toBe('ok');
    expect(snackBadge(shifted(30)).cls).toBe('ok');
  });
});

describe('daysUntil', () => {
  it('returns 0 for today and 1 for tomorrow', () => {
    expect(daysUntil(formatDate(new Date()))).toBe(0);
    expect(daysUntil(shifted(1))).toBe(1);
  });
});

describe('lifecycleBadge', () => {
  it('maps lifecycle status to the contract labels', () => {
    expect(lifecycleBadge('in_stock')).toEqual({ cls: 'in_stock', label: '未打开' });
    expect(lifecycleBadge('opened')).toEqual({ cls: 'opened', label: '已打开' });
    expect(lifecycleBadge('finished')).toEqual({ cls: 'finished', label: '已吃完' });
  });
});

describe('currentLocalMinute', () => {
  it('produces the contract time format YYYY-MM-DDTHH:MM', () => {
    expect(currentLocalMinute()).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/);
  });
});
