import { Injectable, computed, inject, signal } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Router } from '@angular/router';
import { Observable, firstValueFrom, tap } from 'rxjs';
import { LoginResponse, User } from '../models';

const TOKEN_STORAGE_KEY = 'snack-warehouse.token';

/** 认证状态与 /api/auth/* 调用；token 存 localStorage，所有请求由拦截器附加 Bearer 头。 */
@Injectable({ providedIn: 'root' })
export class AuthService {
  private readonly http = inject(HttpClient);
  private readonly router = inject(Router);

  readonly token = signal<string | null>(localStorage.getItem(TOKEN_STORAGE_KEY));
  readonly user = signal<User | null>(null);
  readonly loggedIn = computed(() => this.token() !== null);
  readonly isAdmin = computed(() => this.user()?.is_admin === true);

  private mePromise: Promise<User | null> | null = null;

  login(username: string, password: string): Observable<LoginResponse> {
    return this.http.post<LoginResponse>('/api/auth/login', { username, password }).pipe(
      tap((res) => {
        localStorage.setItem(TOKEN_STORAGE_KEY, res.token);
        this.token.set(res.token);
        this.user.set(res.user);
      }),
    );
  }

  logout(): void {
    if (this.token()) {
      this.http.post<void>('/api/auth/logout', null).subscribe({ error: () => undefined });
    }
    this.clearSession();
    void this.router.navigate(['/login']);
  }

  changePassword(oldPassword: string, newPassword: string): Observable<void> {
    return this.http.post<void>('/api/auth/change-password', {
      old_password: oldPassword,
      new_password: newPassword,
    });
  }

  /** 401 或主动退出时清理本地会话。 */
  clearSession(): void {
    localStorage.removeItem(TOKEN_STORAGE_KEY);
    this.token.set(null);
    this.user.set(null);
    this.mePromise = null;
  }

  /** 确保 user 信号已填充（供 adminGuard / 顶栏使用）；失败时清理会话。 */
  ensureUser(): Promise<User | null> {
    const current = this.user();
    if (current) {
      return Promise.resolve(current);
    }
    if (!this.token()) {
      return Promise.resolve(null);
    }
    if (!this.mePromise) {
      this.mePromise = firstValueFrom(this.http.get<User>('/api/auth/me'))
        .then((user) => {
          this.user.set(user);
          return user;
        })
        .catch(() => {
          this.clearSession();
          return null;
        });
    }
    return this.mePromise;
  }
}
