import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { apiErrorMessage } from '../../api-error';
import { AuthService } from '../../services/auth.service';

@Component({
  selector: 'app-change-password',
  imports: [FormsModule],
  templateUrl: './change-password.html',
  styleUrl: './change-password.css',
})
export class ChangePassword {
  protected readonly oldPassword = signal('');
  protected readonly newPassword = signal('');
  protected readonly confirmPassword = signal('');
  protected readonly error = signal('');
  protected readonly notice = signal('');
  protected readonly submitting = signal(false);

  private readonly auth = inject(AuthService);

  protected submit(): void {
    const oldPassword = this.oldPassword();
    const newPassword = this.newPassword();
    const confirmPassword = this.confirmPassword();
    if (!oldPassword || !newPassword) {
      this.error.set('请输入原密码和新密码');
      return;
    }
    if (newPassword.length < 6) {
      this.error.set('新密码至少 6 位字符');
      return;
    }
    if (newPassword !== confirmPassword) {
      this.error.set('两次输入的新密码不一致');
      return;
    }
    this.error.set('');
    this.notice.set('');
    this.submitting.set(true);
    this.auth.changePassword(oldPassword, newPassword).subscribe({
      next: () => {
        this.submitting.set(false);
        this.oldPassword.set('');
        this.newPassword.set('');
        this.confirmPassword.set('');
        this.notice.set('密码修改成功');
      },
      error: (err: unknown) => {
        this.submitting.set(false);
        this.error.set(apiErrorMessage(err, '密码修改失败'));
      },
    });
  }
}
