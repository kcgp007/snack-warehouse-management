import { Injectable, computed, inject, signal } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Reminder } from '../models';

/** 过期提醒数据源；count 供导航栏与库存页红色角标使用。 */
@Injectable({ providedIn: 'root' })
export class ReminderService {
  private readonly http = inject(HttpClient);

  readonly reminders = signal<Reminder[]>([]);
  readonly count = computed(() => this.reminders().length);
  readonly loading = signal(false);
  readonly error = signal('');

  /** 请求序号：导航/操作触发的并发刷新只接受最后一次结果。 */
  private seq = 0;

  refresh(): void {
    const seq = ++this.seq;
    this.loading.set(true);
    this.http.get<Reminder[]>('/api/reminders').subscribe({
      next: (list) => {
        if (seq !== this.seq) {
          return;
        }
        this.reminders.set(list);
        this.loading.set(false);
        this.error.set('');
      },
      error: () => {
        if (seq !== this.seq) {
          return;
        }
        this.loading.set(false);
        // 401 由拦截器统一处理；这里只提示非鉴权类失败。
        this.error.set('加载过期提醒失败，请稍后重试');
      },
    });
  }

  retry(): void {
    this.error.set('');
    this.refresh();
  }
}
