import { SnackFormModel, formToSnackInput, lifecycleTimes } from './snack-form';

function baseForm(overrides: Partial<SnackFormModel> = {}): SnackFormModel {
  return {
    name: '  薯片  ',
    quantity: '2',
    location: ' 客厅/零食柜 ',
    purchase_date: '2024-01-01',
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
    expect(saved.expiry_date).toBeNull();
  });
});
