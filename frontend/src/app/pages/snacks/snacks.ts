import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { forkJoin, Subject } from 'rxjs';
import { debounceTime, distinctUntilChanged } from 'rxjs/operators';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { apiErrorMessage } from '../../api-error';
import {
  ExpiryStatusFilter,
  LifecycleStatusFilter,
  Snack,
  SnackInput,
} from '../../models';
import { LifecycleBadge, SnackBadge, currentLocalMinute, lifecycleBadge, snackBadgeOf } from '../../snack-status';
import { ReminderService } from '../../services/reminder.service';
import { SnackService } from '../../services/snack.service';
import { SnackFormModel, formToSnackInput, lifecycleTimes } from './snack-form';

const emptyForm = (): SnackFormModel => ({
  name: '',
  quantity: '1',
  location: '',
  purchase_date: '',
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
  protected readonly saving = signal(false);
  protected readonly quickBusy = signal(false);

  protected readonly reminderCount = inject(ReminderService).count;

  private readonly snackService = inject(SnackService);
  private readonly reminders = inject(ReminderService);
  private readonly searchChange = new Subject<string>();

  constructor() {
    this.searchChange
      .pipe(debounceTime(300), distinctUntilChanged(), takeUntilDestroyed())
      .subscribe((value) => {
        this.q.set(value);
        this.load();
      });
    this.load();
  }

  protected load(): void {
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
        this.snacks.set(snacks);
        this.locations.set(locations);
        this.loading.set(false);
        this.error.set('');
      },
      error: (err: unknown) => {
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
    this.notice.set('');
    this.modalOpen.set(true);
  }

  protected openEdit(snack: Snack): void {
    this.editing.set(snack);
    this.form.set({
      name: snack.name,
      quantity: String(snack.quantity),
      location: snack.location,
      purchase_date: snack.purchase_date ?? '',
      expiry_date: snack.expiry_date ?? '',
      notes: snack.notes ?? '',
      status: snack.status,
      opened_at: snack.opened_at ?? '',
      finished_at: snack.finished_at ?? '',
    });
    this.formError.set('');
    this.notice.set('');
    this.modalOpen.set(true);
  }

  protected closeModal(): void {
    this.modalOpen.set(false);
  }

  protected patch(part: Partial<SnackFormModel>): void {
    this.form.update((value) => ({ ...value, ...part }));
  }

  protected onQuantityChange(value: unknown): void {
    this.patch({ quantity: value == null ? '' : String(value) });
  }

  protected save(): void {
    const form = this.form();
    const name = form.name.trim();
    const location = form.location.trim();
    if (!name || !location) {
      this.formError.set('名称和存放位置不能为空');
      return;
    }
    if (form.quantity.trim() === '') {
      this.formError.set('请填写数量');
      return;
    }
    const quantity = Number(form.quantity);
    if (!Number.isInteger(quantity) || quantity < 0) {
      this.formError.set('数量必须是不小于 0 的整数');
      return;
    }
    // 状态/时间一致性归一化（契约 §3）：finished 才带 finished_at（留空则填当前时间），
    // in_stock 不带 opened_at/finished_at —— 例如“已吃完→未打开”保存后不再残留矛盾时间。
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
    this.quickPut(snack, { ...snackToInput(snack), status: 'opened' }, `「${snack.name}」已标记打开`);
  }

  protected markFinished(snack: Snack): void {
    this.quickPut(
      snack,
      { ...snackToInput(snack), status: 'finished', finished_at: currentLocalMinute() },
      `「${snack.name}」已标记吃完`,
    );
  }

  protected remove(snack: Snack): void {
    if (!window.confirm(`确定删除零食「${snack.name}」吗？`)) {
      return;
    }
    this.snackService.remove(snack.id).subscribe({
      next: () => this.afterMutation('零食已删除'),
      error: (err: unknown) => this.error.set(apiErrorMessage(err, '删除失败')),
    });
  }

  protected expiryBadgeOf(snack: Snack): SnackBadge {
    return snackBadgeOf(snack);
  }

  protected lifecycleBadgeOf(snack: Snack): LifecycleBadge {
    return lifecycleBadge(snack.status);
  }

  private quickPut(snack: Snack, input: SnackInput, message: string): void {
    if (this.quickBusy()) {
      return;
    }
    this.quickBusy.set(true);
    this.snackService.update(snack.id, input).subscribe({
      next: () => {
        this.quickBusy.set(false);
        this.afterMutation(message);
      },
      error: (err: unknown) => {
        this.quickBusy.set(false);
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
