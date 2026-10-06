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

  constructor() {
    this.reminders.refresh();
  }

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
