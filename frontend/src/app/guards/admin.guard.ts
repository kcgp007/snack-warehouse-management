import { inject } from '@angular/core';
import { CanActivateFn, Router } from '@angular/router';
import { AuthService } from '../services/auth.service';

/** 保护 /users：仅 is_admin=1 可访问，非管理员跳回首页。 */
export const adminGuard: CanActivateFn = async () => {
  const auth = inject(AuthService);
  const router = inject(Router);
  if (!auth.token()) {
    return router.createUrlTree(['/login']);
  }
  const user = await auth.ensureUser();
  if (!auth.token()) {
    // 401 时拦截器已清理会话并跳转 /login，这里返回 false 避免二次导航。
    return false;
  }
  return user && user.is_admin ? true : router.createUrlTree(['/']);
};
