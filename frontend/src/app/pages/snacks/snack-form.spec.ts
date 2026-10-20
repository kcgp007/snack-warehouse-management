import {
  SnackFormModel,
  addDays,
  changeDateField,
  daysBetween,
  deriveDates,
  formToSnackInput,
  inferLifecycleStatus,
  lifecycleError,
  lifecycleTimes,
  shelfFormValue,
  shelfLifeDays,
  shelfLifeLabel,
} from './snack-form';

function baseForm(overrides: Partial<SnackFormModel> = {}): SnackFormModel {
  return {
    name: '  薯片  ',
    quantity: '2',
    location: ' 客厅/零食柜 ',
    purchase_date: '2024-01-01',
    production_date: '',
    shelf_life_value: '',
    shelf_life_unit: 'day',
    expiry_date: '',
    notes: ' 辣味 ',
    status: 'in_stock',
    opened_at: '',
    finished_at: '',
    ...overrides,
  };
}

const DATE_FORMAT = /^\d{4}-\d{2}-\d{2}$/;

describe('inferLifecycleStatus（状态由打开/吃完日期自动推断）', () => {
  it('两者都空 → 未打开', () => {
    expect(inferLifecycleStatus(null, null)).toBe('in_stock');
    expect(inferLifecycleStatus('', '')).toBe('in_stock');
  });

  it('只有打开日期 → 已打开', () => {
    expect(inferLifecycleStatus('2024-01-02', null)).toBe('opened');
    expect(inferLifecycleStatus('2024-01-02', '')).toBe('opened');
  });

  it('有吃完日期 → 已吃完（无论是否有打开日期）', () => {
    expect(inferLifecycleStatus(null, '2024-01-03')).toBe('finished');
    expect(inferLifecycleStatus('2024-01-02', '2024-01-03')).toBe('finished');
    expect(inferLifecycleStatus('', '2024-01-03')).toBe('finished');
  });
});

describe('lifecycleTimes（按日期推断状态并归一化日期）', () => {
  it('两个日期都为空 → 未打开，两个日期都清空', () => {
    expect(lifecycleTimes(null, null)).toEqual({
      status: 'in_stock',
      opened_at: null,
      finished_at: null,
    });
    expect(lifecycleTimes('', '')).toEqual({
      status: 'in_stock',
      opened_at: null,
      finished_at: null,
    });
  });

  it('两个日期都填 → 已吃完（吃完日期优先）', () => {
    expect(lifecycleTimes('2024-01-02', '2024-01-03')).toEqual({
      status: 'finished',
      opened_at: '2024-01-02',
      finished_at: '2024-01-03',
    });
  });

  it('opened：保留打开日期，清空吃完日期', () => {
    expect(lifecycleTimes('2024-01-02', null)).toEqual({
      status: 'opened',
      opened_at: '2024-01-02',
      finished_at: null,
    });
  });

  it('吃完日期为空 → 不视为已吃完（未填写即为空）', () => {
    expect(lifecycleTimes(null, '')).toEqual({
      status: 'in_stock',
      opened_at: null,
      finished_at: null,
    });
    expect(lifecycleTimes('2024-01-02', '')).toEqual({
      status: 'opened',
      opened_at: '2024-01-02',
      finished_at: null,
    });
  });
});

describe('lifecycleError（打开/吃完日期的前后逻辑）', () => {
  it('两者都为空时不报错', () => {
    expect(lifecycleError(null, null)).toBe('');
    expect(lifecycleError('', '')).toBe('');
  });

  it('只有打开日期时不报错', () => {
    expect(lifecycleError('2024-01-02', null)).toBe('');
  });

  it('只有吃完日期而无打开日期时报错', () => {
    expect(lifecycleError(null, '2024-01-03')).toBe(
      '吃完日期不能早于打开日期：请先填写打开日期',
    );
  });

  it('打开日期晚于吃完日期时报错', () => {
    expect(lifecycleError('2024-01-05', '2024-01-03')).toBe('打开日期不能晚于吃完日期');
  });

  it('打开日期等于吃完日期时不报错', () => {
    expect(lifecycleError('2024-01-03', '2024-01-03')).toBe('');
  });
});

describe('formToSnackInput（状态由日期推断，不再按手动状态归一化）', () => {
  it('两个日期都填 → status finished（吃完日期优先）', () => {
    const saved = formToSnackInput(
      baseForm({
        opened_at: '2024-01-02',
        finished_at: '2024-01-03',
      }),
    );
    expect(saved.status).toBe('finished');
    expect(saved.opened_at).toBe('2024-01-02');
    expect(saved.finished_at).toBe('2024-01-03');
  });

  it('只有打开日期 → status opened，吃完日期清空', () => {
    const saved = formToSnackInput(
      baseForm({
        opened_at: '2024-01-02',
        finished_at: '2024-01-03',
      }),
    );
    // 两个日期都填时 status 为 finished；只保留打开日期的情况由 openEdit 载入后再手动清空吃完日期触发
    expect(saved.status).toBe('finished');
    expect(saved.opened_at).toBe('2024-01-02');

    const openedOnly = formToSnackInput(baseForm({ opened_at: '2024-01-02' }));
    expect(openedOnly.status).toBe('opened');
    expect(openedOnly.opened_at).toBe('2024-01-02');
    expect(openedOnly.finished_at).toBeNull();
  });

  it('只有吃完日期 → status finished，打开日期可为空', () => {
    const saved = formToSnackInput(baseForm({ finished_at: '2024-01-03' }));
    expect(saved.status).toBe('finished');
    expect(saved.opened_at).toBeNull();
    expect(saved.finished_at).toBe('2024-01-03');
  });

  it('两个日期都空 → status in_stock，两个日期都清空', () => {
    const saved = formToSnackInput(baseForm());
    expect(saved.status).toBe('in_stock');
    expect(saved.opened_at).toBeNull();
    expect(saved.finished_at).toBeNull();
  });

  it('只有打开日期 → opened，吃完日期为空', () => {
    const autofilledFinished = formToSnackInput(baseForm({ opened_at: '2024-01-02' }));
    expect(autofilledFinished.status).toBe('opened');
    expect(autofilledFinished.opened_at).toBe('2024-01-02');
    expect(autofilledFinished.finished_at).toBeNull();
  });

  it('填写吃完日期（打开日期可为空） → finished，保留吃完日期', () => {
    const finishedOnly = formToSnackInput(baseForm({ finished_at: '2024-01-03' }));
    expect(finishedOnly.status).toBe('finished');
    expect(finishedOnly.opened_at).toBeNull();
    expect(finishedOnly.finished_at).toBe('2024-01-03');

    const finishedWithOpened = formToSnackInput(baseForm({ opened_at: '2024-01-02', finished_at: '2024-01-03' }));
    expect(finishedWithOpened.status).toBe('finished');
    expect(finishedWithOpened.opened_at).toBe('2024-01-02');
    expect(finishedWithOpened.finished_at).toBe('2024-01-03');
  });

  it('trims text fields and parses quantity', () => {
    const saved = formToSnackInput(baseForm());
    expect(saved.name).toBe('薯片');
    expect(saved.location).toBe('客厅/零食柜');
    expect(saved.notes).toBe('辣味');
    expect(saved.quantity).toBe(2);
    expect(saved.purchase_date).toBe('2024-01-01');
    expect(saved.production_date).toBeNull();
    expect(saved.shelf_life_days).toBeNull();
    expect(saved.shelf_life_unit).toBe('day');
    expect(saved.expiry_date).toBeNull();
  });

  it('converts shelf life value by unit before submitting', () => {
    const days = formToSnackInput(
      baseForm({ production_date: '2026-01-01', shelf_life_value: '30' }),
    );
    expect(days.production_date).toBe('2026-01-01');
    expect(days.shelf_life_days).toBe(30);
    expect(days.shelf_life_unit).toBe('day');
    expect(days.expiry_date).toBeNull();

    const months = formToSnackInput(
      baseForm({ shelf_life_value: '6', shelf_life_unit: 'month' }),
    );
    expect(months.shelf_life_days).toBe(180);
    expect(months.shelf_life_unit).toBe('month');

    const years = formToSnackInput(baseForm({ shelf_life_value: '1', shelf_life_unit: 'year' }));
    expect(years.shelf_life_days).toBe(365);
    expect(years.shelf_life_unit).toBe('year');
  });
});

describe('date helpers', () => {
  it('adds days across month/year boundaries', () => {
    expect(addDays('2026-01-31', 1)).toBe('2026-02-01');
    expect(addDays('2026-12-31', 1)).toBe('2027-01-01');
    expect(addDays('2026-01-01', 0)).toBe('2026-01-01');
  });

  it('computes day differences and rejects invalid dates', () => {
    expect(daysBetween('2026-01-01', '2026-01-31')).toBe(30);
    expect(daysBetween('2026-02-30', '2026-03-01')).toBeNull();
  });
});

describe('shelfLifeDays / shelfLifeLabel', () => {
  it('converts day/month/year with fixed 30/365 factors', () => {
    expect(shelfLifeDays('6', 'day')).toBe(6);
    expect(shelfLifeDays('6', 'month')).toBe(180);
    expect(shelfLifeDays('1', 'year')).toBe(365);
    expect(shelfLifeDays('', 'month')).toBeNull();
    expect(shelfLifeDays('1.5', 'day')).toBeNull();
    expect(shelfLifeDays('-1', 'day')).toBeNull();
    expect(shelfLifeDays('101', 'year')).toBeNull();
  });

  it('restores the saved unit into the form only when divisible', () => {
    expect(shelfFormValue(180, 'month')).toEqual({ value: '6', unit: 'month' });
    expect(shelfFormValue(365, 'year')).toEqual({ value: '1', unit: 'year' });
    expect(shelfFormValue(45, 'month')).toEqual({ value: '45', unit: 'day' });
    expect(shelfFormValue(30, 'day')).toEqual({ value: '30', unit: 'day' });
    expect(shelfFormValue(null, 'month')).toEqual({ value: '', unit: 'month' });
  });

  it('prints back the saved unit when divisible, otherwise days', () => {
    expect(shelfLifeLabel(180, 'month')).toBe('6 个月');
    expect(shelfLifeLabel(365, 'year')).toBe('1 年');
    expect(shelfLifeLabel(45, 'month')).toBe('45 天');
    expect(shelfLifeLabel(45, 'day')).toBe('45 天');
    expect(shelfLifeLabel(null, 'day')).toBe('—');
  });
});

describe('changeDateField（表单字段写回）', () => {
  it('把 shelf_life 输入写入 shelf_life_value 并同步推导过期日期', () => {
    const form = baseForm({ production_date: '2026-01-01', shelf_life_value: '6' });
    expect(changeDateField(form, 'shelf_life', '6')).toEqual({
      shelf_life_value: '6',
      expiry_date: '2026-01-07',
    });
  });

  it('生产日期与保质期两项填写后得到过期日期（回归：字段名映射）', () => {
    const form = baseForm({ production_date: '2026-01-01' });
    const withShelf = { ...form, ...changeDateField(form, 'shelf_life', '30') };
    expect(withShelf.shelf_life_value).toBe('30');
    expect(withShelf.expiry_date).toBe('2026-01-31');
  });
});

describe('deriveDates（三项填两项自动算第三项）', () => {
  it('生产日期 + 保质期 → 过期日期', () => {
    expect(
      deriveDates(
        baseForm({ production_date: '2026-01-01', shelf_life_value: '30' }),
        'production_date',
      ),
    ).toEqual({ expiry_date: '2026-01-31' });
  });

  it('按月换算：生产日期 + 6 个月 → 过期日期', () => {
    expect(
      deriveDates(
        baseForm({
          production_date: '2026-01-01',
          shelf_life_value: '6',
          shelf_life_unit: 'month',
        }),
        'production_date',
      ),
    ).toEqual({ expiry_date: '2026-06-30' });
  });

  it('生产日期 + 过期日期 → 保质期（回退为天）', () => {
    expect(
      deriveDates(
        baseForm({ production_date: '2026-01-01', expiry_date: '2026-03-02' }),
        'production_date',
      ),
    ).toEqual({ shelf_life_value: '60', shelf_life_unit: 'day' });
  });

  it('保质期 + 过期日期 → 生产日期', () => {
    expect(
      deriveDates(
        baseForm({ shelf_life_value: '30', expiry_date: '2026-01-31' }),
        'expiry_date',
      ),
    ).toEqual({ production_date: '2026-01-01' });
  });

  it('修改生产日期时保留保质期并重算过期日期', () => {
    expect(
      deriveDates(
        baseForm({
          production_date: '2026-02-01',
          shelf_life_value: '10',
          expiry_date: '2026-01-11',
        }),
        'production_date',
      ),
    ).toEqual({ expiry_date: '2026-02-11' });
  });

  it('修改过期日期时保留生产日期并重算保质期', () => {
    expect(
      deriveDates(
        baseForm({
          production_date: '2026-01-01',
          shelf_life_value: '10',
          expiry_date: '2026-01-21',
        }),
        'expiry_date',
      ),
    ).toEqual({ shelf_life_value: '20', shelf_life_unit: 'day' });
  });

  it('修改保质期时保留生产日期并重算过期日期', () => {
    expect(
      deriveDates(
        baseForm({
          production_date: '2026-01-01',
          shelf_life_value: '45',
          expiry_date: '2026-01-11',
        }),
        'shelf_life',
      ),
    ).toEqual({ expiry_date: '2026-02-15' });
  });

  it('切换单位时保留数值并按新单位重算过期日期', () => {
    expect(
      deriveDates(
        baseForm({
          production_date: '2026-01-01',
          shelf_life_value: '6',
          shelf_life_unit: 'month',
        }),
        'shelf_life',
      ),
    ).toEqual({ expiry_date: '2026-06-30' });
  });

  it('只有一项或不完整/非法时不计算', () => {
    expect(
      deriveDates(baseForm({ production_date: '2026-01-01' }), 'production_date'),
    ).toEqual({});
    expect(
      deriveDates(
        baseForm({ shelf_life_value: '-1', expiry_date: '2026-01-31' }),
        'expiry_date',
      ),
    ).toEqual({});
    expect(
      deriveDates(
        baseForm({ production_date: '2026-02-30', shelf_life_value: '10' }),
        'production_date',
      ),
    ).toEqual({});
  });

  it('过期日期早于生产日期时不生成负数保质期', () => {
    expect(
      deriveDates(
        baseForm({ production_date: '2026-02-01', expiry_date: '2026-01-01' }),
        'production_date',
      ),
    ).toEqual({});
  });
});
