import { Injectable, computed, inject, signal } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Reminder } from '../models';

/** 过期提醒数据源；count 供导航栏与库存页红色角标使用。 */
@Injectable({ providedIn: 'root' })
export class ReminderService {
  private readonly http = inject(HttpClient);

  readonly reminders = signal<Reminder[]>([]);
  readonly count = computed(() => this.reminders().length);

  refresh(): void {
    this.http.get<Reminder[]>('/api/reminders').subscribe({
      next: (list) => this.reminders.set(list),
      error: () => undefined, // 401 由拦截器统一处理
    });
  }
}
