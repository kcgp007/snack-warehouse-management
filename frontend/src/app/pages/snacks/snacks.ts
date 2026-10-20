import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { forkJoin, of, Subject } from 'rxjs';
import { catchError, debounceTime, distinctUntilChanged, switchMap } from 'rxjs/operators';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { apiErrorMessage } from '../../api-error';
import {
  ExpiryStatusFilter,
  LifecycleStatusFilter,
  ShelfLifeUnit,
  Snack,
  SnackInput,
} from '../../models';
import {
  LifecycleBadge,
  SnackBadge,
  currentLocalDate,
  lifecycleBadge,
  snackBadgeOf,
} from '../../snack-status';
import { ReminderService } from '../../services/reminder.service';
import { SnackService } from '../../services/snack.service';
import {
  LifecycleDateField,
  SnackFormModel,
  changeDateField,
  deriveDates,
  formToSnackInput,
  lifecycleTimes,
  shelfFormValue,
  shelfLifeDays,
  shelfLifeLabel,
} from './snack-form';

const emptyForm = (): SnackFormModel => ({
  name: '',
  quantity: '1',
  location: '',
  purchase_date: '',
  production_date: '',
  shelf_life_value: '',
  shelf_life_unit: 'day',
  expiry_date: '',
  notes: '',
  status: 'in_stock',
  opened_at: '',
  finished_at: '',
});

/**
 * Snack → SnackInput（用于快捷动作：保留全部既有字段，仅改 status 相关字段）。
 * 时间字段同样走 `lifecycleTimes`，避免快捷动作把矛盾数据原样写回。
 */
function snackToInput(snack: Snack): SnackInput {
  return {
    name: snack.name,
    quantity: snack.quantity,
    location: snack.location,
    purchase_date: snack.purchase_date,
    production_date: snack.production_date,
    shelf_life_days: snack.shelf_life_days,
    shelf_life_unit: snack.shelf_life_unit,
    expiry_date: snack.expiry_date,
    notes: snack.notes,
    status: snack.status,
    ...lifecycleTimes(snack.status, snack.opened_at, snack.finished_at),
  };
}

@Component({
  selector: 'app-snacks',
  imports: [FormsModule, RouterLink],
  templateUrl: './snacks.html',
  styleUrl: './snacks.css',
})
export class Snacks {
  protected readonly snacks = signal<Snack[]>([]);
  protected readonly locations = signal<string[]>([]);
  protected readonly loading = signal(false);
  protected readonly error = signal('');
  protected readonly notice = signal('');

  protected readonly q = signal('');
  protected readonly locationFilter = signal('');
  protected readonly expiryStatusFilter = signal<ExpiryStatusFilter>('all');
  protected readonly lifecycleFilter = signal<LifecycleStatusFilter>('all');

  protected readonly modalOpen = signal(false);
  protected readonly editing = signal<Snack | null>(null);
  protected readonly form = signal<SnackFormModel>(emptyForm());
  protected readonly formError = signal('');
  protected readonly touched = signal<
    Record<'name' | 'quantity' | 'location' | 'shelf_life_value', boolean>
  >({
    name: false,
    quantity: false,
    location: false,
    shelf_life_value: false,
  });
  protected readonly saving = signal(false);
  /** 正在执行快捷动作的零食 id（仅禁用该行按钮，其他行仍可操作）。 */
  protected readonly quickBusyId = signal<number | null>(null);
  /** 待删除的零食（非空时展示删除确认弹窗，替代 window.confirm）。 */
  protected readonly pendingDelete = signal<Snack | null>(null);

  /** 位置输入自动补全（契约 v2）：按已输入字符防抖请求 GET /api/locations?q=。 */
  protected readonly locationSuggestions = signal<string[]>([]);
  protected readonly suggestionsOpen = signal(false);
  protected readonly activeSuggestion = signal(-1);

  protected readonly reminderCount = inject(ReminderService).count;

  private readonly snackService = inject(SnackService);
  private readonly reminders = inject(ReminderService);
  private readonly searchChange = new Subject<string>();
  private readonly locationInput = new Subject<string>();
  /** 列表请求序号：筛选快速切换时丢弃过期响应，避免旧数据覆盖新数据。 */
  private loadSeq = 0;

  constructor() {
    this.searchChange
      .pipe(debounceTime(300), distinctUntilChanged(), takeUntilDestroyed())
      .subscribe((value) => {
        this.q.set(value);
        this.load();
      });
    this.locationInput
      .pipe(
        debounceTime(250),
        switchMap((term) =>
          this.snackService
            .locations(term)
            .pipe(catchError(() => of<string[]>([]))),
        ),
        takeUntilDestroyed(),
      )
      .subscribe((locations) => this.locationSuggestions.set(locations));
    this.load();
  }

  protected load(): void {
    const seq = ++this.loadSeq;
    this.loading.set(true);
    forkJoin({
      snacks: this.snackService.list({
        q: this.q(),
        location: this.locationFilter(),
        expiryStatus: this.expiryStatusFilter(),
        status: this.lifecycleFilter(),
      }),
      locations: this.snackService.locations(),
    }).subscribe({
      next: ({ snacks, locations }) => {
        if (seq !== this.loadSeq) {
          return;
        }
        this.snacks.set(snacks);
        this.locations.set(locations);
        this.loading.set(false);
        this.error.set('');
      },
      error: (err: unknown) => {
        if (seq !== this.loadSeq) {
          return;
        }
        this.loading.set(false);
        this.error.set(apiErrorMessage(err, '加载零食列表失败'));
      },
    });
  }

  protected onSearch(value: string): void {
    this.notice.set('');
    this.searchChange.next(value);
  }

  protected onLocationChange(value: string): void {
    this.notice.set('');
    this.locationFilter.set(value);
    this.load();
  }

  protected onExpiryStatusChange(value: string): void {
    this.notice.set('');
    this.expiryStatusFilter.set(value as ExpiryStatusFilter);
    this.load();
  }

  protected onLifecycleChange(value: string): void {
    this.notice.set('');
    this.lifecycleFilter.set(value as LifecycleStatusFilter);
    this.load();
  }

  protected openCreate(): void {
    this.editing.set(null);
    this.form.set(emptyForm());
    this.formError.set('');
    this.touched.set({
      name: false,
      quantity: false,
      location: false,
      shelf_life_value: false,
    });
    this.notice.set('');
    this.resetLocationSuggestions();
    this.modalOpen.set(true);
  }

  protected openEdit(snack: Snack): void {
    this.editing.set(snack);
    const shelf = shelfFormValue(snack.shelf_life_days, snack.shelf_life_unit);
    this.form.set({
      name: snack.name,
      quantity: String(snack.quantity),
      location: snack.location,
      purchase_date: snack.purchase_date ?? '',
      production_date: snack.production_date ?? '',
      shelf_life_value: shelf.value,
      shelf_life_unit: shelf.unit,
      expiry_date: snack.expiry_date ?? '',
      notes: snack.notes ?? '',
      status: snack.status,
      opened_at: snack.opened_at ?? '',
      finished_at: snack.finished_at ?? '',
    });
    this.formError.set('');
    this.touched.set({
      name: false,
      quantity: false,
      location: false,
      shelf_life_value: false,
    });
    this.notice.set('');
    this.resetLocationSuggestions();
    this.modalOpen.set(true);
  }

  protected closeModal(): void {
    this.resetLocationSuggestions();
    this.modalOpen.set(false);
  }

  protected patch(part: Partial<SnackFormModel>): void {
    this.form.update((value) => ({ ...value, ...part }));
  }

  /** 位置输入：更新表单并按字符请求补全建议。 */
  protected onLocationInput(value: string): void {
    this.patch({ location: value });
    this.activeSuggestion.set(-1);
    this.suggestionsOpen.set(true);
    this.locationInput.next(value);
  }

  /** 聚焦位置输入框时展示已有位置（空串 = 全部）。 */
  protected onLocationFocus(): void {
    this.activeSuggestion.set(-1);
    this.suggestionsOpen.set(true);
    this.locationInput.next(this.form().location);
  }

  protected onLocationBlur(): void {
    this.suggestionsOpen.set(false);
    this.activeSuggestion.set(-1);
  }

  /** 键盘导航：↑/↓ 选择，Enter 确认，Esc 关闭。 */
  protected onLocationKeydown(event: KeyboardEvent): void {
    if (!this.suggestionsOpen()) {
      return;
    }
    const count = this.locationSuggestions().length;
    if (event.key === 'Escape') {
      this.onLocationBlur();
      return;
    }
    if (count === 0) {
      return;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      this.activeSuggestion.update((index) => (index + 1) % count);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      this.activeSuggestion.update((index) => (index <= 0 ? count - 1 : index - 1));
    } else if (event.key === 'Enter') {
      const active = this.activeSuggestion();
      if (active >= 0) {
        event.preventDefault();
        this.pickLocation(this.locationSuggestions()[active]);
      }
    }
  }

  /** 点击建议只回填位置，仍允许自由输入新位置。 */
  protected pickLocation(location: string): void {
    this.patch({ location });
    this.resetLocationSuggestions();
  }

  private resetLocationSuggestions(): void {
    this.suggestionsOpen.set(false);
    this.activeSuggestion.set(-1);
    this.locationSuggestions.set([]);
  }

  protected onQuantityChange(value: unknown): void {
    this.patch({ quantity: value == null ? '' : String(value) });
  }

  /**
   * 生产日期 / 保质期 / 过期日期联动：以本次修改的字段为基准，
   * 填满任意两项后自动计算第三项（见 `deriveDates`）。
   */
  protected onLifecycleDateChange(field: LifecycleDateField, value: string): void {
    this.patch(changeDateField(this.form(), field, value));
  }

  protected onShelfLifeChange(value: unknown): void {
    this.onLifecycleDateChange('shelf_life', value == null ? '' : String(value));
  }

  /** 切换保质期单位：保留数值、按新单位重算第三项。 */
  protected onShelfLifeUnitChange(unit: ShelfLifeUnit): void {
    const next = { ...this.form(), shelf_life_unit: unit };
    this.patch({ shelf_life_unit: unit, ...deriveDates(next, 'shelf_life') });
  }

  /** 字段失焦后展示该字段的即时校验提示。 */
  protected markTouched(
    field: 'name' | 'quantity' | 'location' | 'shelf_life_value',
  ): void {
    this.touched.update((value) => ({ ...value, [field]: true }));
  }

  /** 即时字段错误；空串表示通过。 */
  protected fieldError(
    field: 'name' | 'quantity' | 'location' | 'shelf_life_value',
  ): string {
    const form = this.form();
    if (field === 'name' && !form.name.trim()) {
      return '请输入零食名称';
    }
    if (field === 'location' && !form.location.trim()) {
      return '请输入存放位置';
    }
    if (field === 'shelf_life_value') {
      const raw = form.shelf_life_value.trim();
      if (!raw) {
        return '';
      }
      const amount = Number(raw);
      if (!Number.isInteger(amount) || amount < 0) {
        return '保质期必须是不小于 0 的整数';
      }
      if (shelfLifeDays(form.shelf_life_value, form.shelf_life_unit) === null) {
        return '保质期换算后不能超过 36500 天';
      }
    }
    if (field === 'quantity') {
      const raw = form.quantity.trim();
      if (!raw) {
        return '请填写数量';
      }
      const quantity = Number(raw);
      if (!Number.isInteger(quantity) || quantity < 0) {
        return '数量必须是不小于 0 的整数';
      }
    }
    return '';
  }

  /** 表格中的保质期展示：按保存单位回显（如 6 个月），否则显示天数。 */
  protected shelfLifeLabelOf(snack: Snack): string {
    return shelfLifeLabel(snack.shelf_life_days, snack.shelf_life_unit);
  }

  /** 生产日期与过期日期的跨字段校验。 */
  protected dateRangeError(): string {
    const form = this.form();
    if (!form.production_date || !form.expiry_date) {
      return '';
    }
    const production = Date.parse(`${form.production_date}T00:00:00Z`);
    const expiry = Date.parse(`${form.expiry_date}T00:00:00Z`);
    return expiry < production ? '过期日期不能早于生产日期' : '';
  }

  protected save(): void {
    const form = this.form();
    this.formError.set('');
    this.touched.set({
      name: true,
      quantity: true,
      location: true,
      shelf_life_value: true,
    });
    const invalid = (
      ['name', 'quantity', 'location', 'shelf_life_value'] as const
    ).find((field) => this.fieldError(field) !== '');
    if (invalid || this.dateRangeError()) {
      return;
    }
    // 状态/日期一致性归一化（契约 §3）：finished 才带 finished_at（留空则填当前日期），
    // in_stock 不带 opened_at/finished_at —— 例如“已吃完→未打开”保存后不再残留矛盾日期。
    const input: SnackInput = formToSnackInput(form);
    const editing = this.editing();
    this.saving.set(true);
    this.formError.set('');
    const request = editing
      ? this.snackService.update(editing.id, input)
      : this.snackService.create(input);
    request.subscribe({
      next: () => {
        this.saving.set(false);
        this.modalOpen.set(false);
        this.afterMutation(editing ? '零食已更新' : '零食已添加');
      },
      error: (err: unknown) => {
        this.saving.set(false);
        this.formError.set(apiErrorMessage(err, '保存失败'));
      },
    });
  }

  /** 快捷动作：一次 PUT 完成，保留其他字段。 */
  protected markOpened(snack: Snack): void {
    this.quickPut(
      snack,
      { ...snackToInput(snack), status: 'opened', opened_at: currentLocalDate() },
      `「${snack.name}」已标记打开`,
    );
  }

  protected markFinished(snack: Snack): void {
    this.quickPut(
      snack,
      { ...snackToInput(snack), status: 'finished', finished_at: currentLocalDate() },
      `「${snack.name}」已标记吃完`,
    );
  }

  protected remove(snack: Snack): void {
    this.pendingDelete.set(snack);
  }

  protected cancelDelete(): void {
    this.pendingDelete.set(null);
  }

  protected confirmDelete(): void {
    const snack = this.pendingDelete();
    if (!snack || this.quickBusyId() !== null) {
      return;
    }
    this.quickBusyId.set(snack.id);
    this.snackService.remove(snack.id).subscribe({
      next: () => {
        this.quickBusyId.set(null);
        this.pendingDelete.set(null);
        this.afterMutation('零食已删除');
      },
      error: (err: unknown) => {
        this.quickBusyId.set(null);
        this.pendingDelete.set(null);
        this.error.set(apiErrorMessage(err, '删除失败'));
      },
    });
  }

  protected expiryBadgeOf(snack: Snack): SnackBadge {
    return snackBadgeOf(snack);
  }

  protected lifecycleBadgeOf(snack: Snack): LifecycleBadge {
    return lifecycleBadge(snack.status);
  }

  /** 按生命周期状态屏蔽矛盾日期：未打开不显示任何日期，吃完日期仅在已吃完时显示。 */
  protected dateCellOf(snack: Snack, kind: 'opened' | 'finished'): string {
    const value = kind === 'finished' ? snack.finished_at : snack.opened_at;
    if (kind === 'finished' && snack.status !== 'finished') {
      return '—';
    }
    if (kind === 'opened' && snack.status === 'in_stock') {
      return '—';
    }
    // 契约 v2 为纯日期；slice 同时兼容启动迁移前的 v1 datetime。
    return value ? value.slice(0, 10) : '—';
  }

  private quickPut(snack: Snack, input: SnackInput, message: string): void {
    if (this.quickBusyId() !== null) {
      return;
    }
    this.quickBusyId.set(snack.id);
    this.snackService.update(snack.id, input).subscribe({
      next: () => {
        this.quickBusyId.set(null);
        this.afterMutation(message);
      },
      error: (err: unknown) => {
        this.quickBusyId.set(null);
        this.error.set(apiErrorMessage(err, '操作失败'));
      },
    });
  }

  private afterMutation(message: string): void {
    this.error.set('');
    this.notice.set(message);
    this.load();
    this.reminders.refresh();
  }
}
