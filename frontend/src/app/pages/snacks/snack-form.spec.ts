import {
  SnackFormModel,
  addDays,
  changeDateField,
  daysBetween,
  deriveDates,
  formToSnackInput,
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

describe('lifecycleTimes', () => {
  it('clears both dates for in_stock（未打开不得带打开/吃完日期）', () => {
    expect(lifecycleTimes('in_stock', '2024-01-02', '2024-01-03')).toEqual({
      opened_at: null,
      finished_at: null,
    });
  });

  it('keeps opened_at and clears finished_at for opened（已打开不得带吃完日期）', () => {
    expect(lifecycleTimes('opened', '2024-01-02', '2024-01-03')).toEqual({
      opened_at: '2024-01-02',
      finished_at: null,
    });
  });

  it('keeps both dates for finished（已吃完保留打开与吃完日期）', () => {
    expect(lifecycleTimes('finished', '2024-01-02', '2024-01-03')).toEqual({
      opened_at: '2024-01-02',
      finished_at: '2024-01-03',
    });
  });

  it('autofills finished_at for finished when it is empty（纯日期）', () => {
    const result = lifecycleTimes('finished', null, '');
    expect(result.opened_at).toBeNull();
    expect(result.finished_at).toMatch(DATE_FORMAT);
    expect(result.finished_at).not.toContain('T');
  });

  it('normalizes empty strings to null', () => {
    expect(lifecycleTimes('opened', '', '')).toEqual({ opened_at: null, finished_at: null });
  });
});

describe('formToSnackInput', () => {
  it('drops stale dates when editing 已吃完 → 未打开（表格不再出现矛盾行）', () => {
    // openEdit 从 finished 零食载入的表单：两个日期都已填写
    const form = baseForm({
      status: 'finished',
      opened_at: '2024-01-02',
      finished_at: '2024-01-03',
    });
    const saved = formToSnackInput({ ...form, status: 'in_stock' });
    expect(saved.status).toBe('in_stock');
    expect(saved.opened_at).toBeNull();
    expect(saved.finished_at).toBeNull();
  });

  it('keeps opened_at but never forwards finished_at for opened', () => {
    const saved = formToSnackInput(
      baseForm({
        status: 'opened',
        opened_at: '2024-01-02',
        finished_at: '2024-01-03',
      }),
    );
    expect(saved.opened_at).toBe('2024-01-02');
    expect(saved.finished_at).toBeNull();
  });

  it('carries finished_at only for finished and autofills it when empty', () => {
    const kept = formToSnackInput(
      baseForm({
        status: 'finished',
        opened_at: '2024-01-02',
        finished_at: '2024-01-03',
      }),
    );
    expect(kept.finished_at).toBe('2024-01-03');

    const autofilled = formToSnackInput(baseForm({ status: 'finished' }));
    expect(autofilled.finished_at).toMatch(DATE_FORMAT);
    expect(autofilled.finished_at).not.toContain('T');
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
