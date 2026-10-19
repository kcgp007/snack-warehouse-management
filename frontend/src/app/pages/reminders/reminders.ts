import { Component, computed, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { groupRemindersByLevel, ReminderGroup } from '../../reminder-group';
import { ReminderService } from '../../services/reminder.service';

@Component({
  selector: 'app-reminders',
  imports: [RouterLink],
  templateUrl: './reminders.html',
  styleUrl: './reminders.css',
})
export class Reminders {
  protected readonly reminders = inject(ReminderService);

  /** 按分级 level（expired/critical/warning）分组，空组不展示。 */
  protected readonly groups = computed<ReminderGroup[]>(() =>
    groupRemindersByLevel(this.reminders.reminders()),
  );

  // 刷新由 App 的 NavigationEnd 统一触发，避免进入页面时重复请求。

  protected daysText(daysLeft: number): string {
    if (daysLeft < 0) {
      return `已过期 ${-daysLeft} 天`;
    }
    if (daysLeft === 0) {
      return '今天到期';
    }
    return `剩余 ${daysLeft} 天`;
  }
}
