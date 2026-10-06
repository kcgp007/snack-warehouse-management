import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { apiErrorMessage } from '../../api-error';
import { AuthService } from '../../services/auth.service';

@Component({
  selector: 'app-login',
  imports: [FormsModule],
  templateUrl: './login.html',
  styleUrl: './login.css',
})
export class Login {
  protected readonly username = signal('');
  protected readonly password = signal('');
  protected readonly error = signal('');
  protected readonly submitting = signal(false);

  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);

  constructor() {
    // 已持有有效 token 时直接进入首页；token 失效则留在登录页。
    if (this.auth.token()) {
      void this.auth.ensureUser().then((user) => {
        if (user) {
          void this.router.navigate(['/']);
        }
      });
    }
  }

  protected submit(): void {
    const username = this.username().trim();
    const password = this.password();
    if (!username || !password) {
      this.error.set('请输入用户名和密码');
      return;
    }
    this.error.set('');
    this.submitting.set(true);
    this.auth.login(username, password).subscribe({
      next: () => {
        this.submitting.set(false);
        void this.router.navigate(['/']);
      },
      error: (err: unknown) => {
        this.submitting.set(false);
        this.error.set(apiErrorMessage(err, '登录失败，请检查用户名和密码'));
      },
    });
  }
}
