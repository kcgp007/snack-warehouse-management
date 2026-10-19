import { inject } from '@angular/core';
import { HttpErrorResponse, HttpInterceptorFn } from '@angular/common/http';
import { Router } from '@angular/router';
import { catchError, throwError } from 'rxjs';
import { AuthService } from '../services/auth.service';

/**
 * 统一附加 `Authorization: Bearer <token>`；收到 401（登录接口除外）时
 * 清理本地 token 并跳转 /login。
 */
export const authInterceptor: HttpInterceptorFn = (req, next) => {
  const auth = inject(AuthService);
  const router = inject(Router);

  const isLoginRequest = req.url.includes('/api/auth/login');
  const token = auth.token();
  const outgoing =
    token && !isLoginRequest
      ? req.clone({ setHeaders: { Authorization: `Bearer ${token}` } })
      : req;

  return next(outgoing).pipe(
    catchError((err: unknown) => {
      if (err instanceof HttpErrorResponse && err.status === 401 && !isLoginRequest) {
        // 并发 401 只跳转一次：首个响应清掉 token 后，后续响应不再重复导航。
        const hadSession = auth.token() !== null;
        auth.clearSession();
        if (hadSession && !router.url.startsWith('/login')) {
          void router.navigate(['/login']);
        }
      }
      return throwError(() => err);
    }),
  );
};
