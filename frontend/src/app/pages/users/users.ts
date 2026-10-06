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
    const username = this.newUsername().trim();
    const password = this.newPassword();
    if (!username || !password) {
      this.error.set('请输入用户名和密码');
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
        this.notice.set(`已添加账户「${user.username}」`);
        this.load();
      },
      error: (err: unknown) => {
        this.adding.set(false);
        this.error.set(apiErrorMessage(err, '添加账户失败'));
      },
    });
  }

  protected remove(user: User): void {
    if (!window.confirm(`确定删除账户「${user.username}」吗？`)) {
      return;
    }
    this.error.set('');
    this.notice.set('');
    this.userService.remove(user.id).subscribe({
      next: () => {
        this.notice.set(`已删除账户「${user.username}」`);
        this.load();
      },
      error: (err: unknown) => {
        // 服务端会拒绝“不能删除当前登录账户”，这里原样展示错误信息
        this.error.set(apiErrorMessage(err, '删除账户失败'));
      },
    });
  }
}
