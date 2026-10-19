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
  protected readonly touched = signal({
    oldPassword: false,
    newPassword: false,
    confirmPassword: false,
  });

  private readonly auth = inject(AuthService);

  protected markTouched(field: 'oldPassword' | 'newPassword' | 'confirmPassword'): void {
    this.touched.update((value) => ({ ...value, [field]: true }));
  }

  protected fieldError(field: 'oldPassword' | 'newPassword' | 'confirmPassword'): string {
    if (field === 'oldPassword' && !this.oldPassword()) {
      return '请输入原密码';
    }
    if (field === 'newPassword') {
      if (!this.newPassword()) {
        return '请输入新密码';
      }
      if (this.newPassword().length < 6) {
        return '新密码至少 6 位字符';
      }
    }
    if (field === 'confirmPassword') {
      if (!this.confirmPassword()) {
        return '请再次输入新密码';
      }
      if (this.newPassword() !== this.confirmPassword()) {
        return '两次输入的新密码不一致';
      }
    }
    return '';
  }

  protected submit(): void {
    const oldPassword = this.oldPassword();
    const newPassword = this.newPassword();
    this.touched.set({ oldPassword: true, newPassword: true, confirmPassword: true });
    const invalid = (['oldPassword', 'newPassword', 'confirmPassword'] as const).find(
      (field) => this.fieldError(field) !== '',
    );
    if (invalid) {
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
