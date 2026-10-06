import { Component, computed, inject, signal } from '@angular/core';
import { NavigationEnd, Router, RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { filter } from 'rxjs';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { AuthService } from './services/auth.service';
import { ReminderService } from './services/reminder.service';

@Component({
  selector: 'app-root',
  imports: [RouterOutlet, RouterLink, RouterLinkActive],
  styleUrl: './app.css',
  templateUrl: './app.html',
})
export class App {
  protected readonly auth = inject(AuthService);
  protected readonly reminders = inject(ReminderService);
  private readonly router = inject(Router);

  private readonly url = signal(window.location.pathname);
  /** 登录页不显示顶栏导航。 */
  protected readonly showNav = computed(() => !this.url().startsWith('/login'));

  constructor() {
    this.router.events
      .pipe(
        filter((event): event is NavigationEnd => event instanceof NavigationEnd),
        takeUntilDestroyed(),
      )
      .subscribe((event) => {
        this.url.set(event.urlAfterRedirects);
        if (this.auth.token()) {
          void this.auth.ensureUser();
          this.reminders.refresh();
        }
      });
  }

  protected logout(): void {
    this.auth.logout();
  }
}
