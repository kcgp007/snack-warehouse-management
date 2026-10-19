import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { apiErrorMessage } from '../../api-error';
import { User } from '../../models';
import { AuthService } from '../../services/auth.service';
import { UserService } from '../../services/user.service';

@Component({
  selector: 'app-users',
  imports: [FormsModule],
  templateUrl: './users.html',
  styleUrl: './users.css',
})
export class Users {
  protected readonly users = signal<User[]>([]);
  protected readonly loading = signal(false);
  protected readonly error = signal('');
  protected readonly notice = signal('');

  protected readonly newUsername = signal('');
  protected readonly newPassword = signal('');
  protected readonly adding = signal(false);
  protected readonly removing = signal(false);
  /** 逐字段校验：blur 后才显示错误，提交时统一标记。 */
  protected readonly touched = signal<Record<'username' | 'password', boolean>>({
    username: false,
    password: false,
  });
  /** 待删除的账户（非空时展示删除确认弹窗，替代 window.confirm）。 */
  protected readonly pendingDelete = signal<User | null>(null);

  protected readonly currentUserId = inject(AuthService).user;

  private readonly userService = inject(UserService);

  constructor() {
    this.load();
  }

  protected load(): void {
    this.loading.set(true);
    this.userService.list().subscribe({
      next: (list) => {
        this.users.set(list);
        this.loading.set(false);
        this.error.set('');
      },
      error: (err: unknown) => {
        this.loading.set(false);
        this.error.set(apiErrorMessage(err, '加载用户列表失败'));
      },
    });
  }

  protected add(): void {
    this.touched.set({ username: true, password: true });
    const username = this.newUsername().trim();
    const password = this.newPassword();
    if (!username || !password) {
      return;
    }
    this.error.set('');
    this.notice.set('');
    this.adding.set(true);
    this.userService.create(username, password).subscribe({
      next: (user) => {
        this.adding.set(false);
        this.newUsername.set('');
        this.newPassword.set('');
        this.touched.set({ username: false, password: false });
        this.notice.set(`已添加账户「${user.username}」`);
        this.load();
      },
      error: (err: unknown) => {
        this.adding.set(false);
        this.error.set(apiErrorMessage(err, '添加账户失败'));
      },
    });
  }

  protected markTouched(field: 'username' | 'password'): void {
    this.touched.update((t) => ({ ...t, [field]: true }));
  }

  /** 当前字段的即时校验错误；无错误返回 ''。 */
  protected fieldError(field: 'username' | 'password'): string {
    if (field === 'username' && !this.newUsername().trim()) {
      return '请输入用户名';
    }
    if (field === 'password' && !this.newPassword()) {
      return '请输入初始密码';
    }
    return '';
  }

  /** RFC3339 → 本地可读时间（用户页不再展示原始 UTC 字符串）。 */
  protected displayDate(value: string | null | undefined): string {
    if (!value) {
      return '—';
    }
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) {
      return value;
    }
    return date.toLocaleString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    });
  }

  protected remove(user: User): void {
    this.pendingDelete.set(user);
  }

  protected cancelDelete(): void {
    this.pendingDelete.set(null);
  }

  protected confirmDelete(): void {
    const user = this.pendingDelete();
    if (!user || this.removing()) {
      return;
    }
    this.removing.set(true);
    this.error.set('');
    this.notice.set('');
    this.userService.remove(user.id).subscribe({
      next: () => {
        this.removing.set(false);
        this.pendingDelete.set(null);
        this.notice.set(`已删除账户「${user.username}」`);
        this.load();
      },
      error: (err: unknown) => {
        this.removing.set(false);
        this.pendingDelete.set(null);
        // 服务端会拒绝“不能删除当前登录账户”，这里原样展示错误信息
        this.error.set(apiErrorMessage(err, '删除账户失败'));
      },
    });
  }
}
