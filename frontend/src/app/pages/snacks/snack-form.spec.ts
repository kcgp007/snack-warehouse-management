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

const TIME_FORMAT = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/;

describe('lifecycleTimes', () => {
  it('clears both timestamps for in_stock（未打开不得带打开/吃完时间）', () => {
    expect(lifecycleTimes('in_stock', '2024-01-02T10:00', '2024-01-03T11:00')).toEqual({
      opened_at: null,
      finished_at: null,
    });
  });

  it('keeps opened_at and clears finished_at for opened（已打开不得带吃完时间）', () => {
    expect(lifecycleTimes('opened', '2024-01-02T10:00', '2024-01-03T11:00')).toEqual({
      opened_at: '2024-01-02T10:00',
      finished_at: null,
    });
  });

  it('keeps both timestamps for finished（已吃完保留打开与吃完时间）', () => {
    expect(lifecycleTimes('finished', '2024-01-02T10:00', '2024-01-03T11:00')).toEqual({
      opened_at: '2024-01-02T10:00',
      finished_at: '2024-01-03T11:00',
    });
  });

  it('autofills finished_at for finished when it is empty', () => {
    const result = lifecycleTimes('finished', null, '');
    expect(result.opened_at).toBeNull();
    expect(result.finished_at).toMatch(TIME_FORMAT);
  });

  it('normalizes empty strings to null', () => {
    expect(lifecycleTimes('opened', '', '')).toEqual({ opened_at: null, finished_at: null });
  });
});

describe('formToSnackInput', () => {
  it('drops stale timestamps when editing 已吃完 → 未打开（表格不再出现矛盾行）', () => {
    // openEdit 从 finished 零食载入的表单：两个时间都已填写
    const form = baseForm({
      status: 'finished',
      opened_at: '2024-01-02T10:00',
      finished_at: '2024-01-03T11:00',
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
        opened_at: '2024-01-02T10:00',
        finished_at: '2024-01-03T11:00',
      }),
    );
    expect(saved.opened_at).toBe('2024-01-02T10:00');
    expect(saved.finished_at).toBeNull();
  });

  it('carries finished_at only for finished and autofills it when empty', () => {
    const kept = formToSnackInput(
      baseForm({
        status: 'finished',
        opened_at: '2024-01-02T10:00',
        finished_at: '2024-01-03T11:00',
      }),
    );
    expect(kept.finished_at).toBe('2024-01-03T11:00');

    const autofilled = formToSnackInput(baseForm({ status: 'finished' }));
    expect(autofilled.finished_at).toMatch(TIME_FORMAT);
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
