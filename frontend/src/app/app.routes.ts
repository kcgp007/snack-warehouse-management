import { Routes } from '@angular/router';
import { adminGuard } from './guards/admin.guard';
import { authGuard } from './guards/auth.guard';
import { ChangePassword } from './pages/change-password/change-password';
import { Login } from './pages/login/login';
import { Reminders } from './pages/reminders/reminders';
import { Snacks } from './pages/snacks/snacks';
import { Users } from './pages/users/users';

export const routes: Routes = [
  { path: 'login', component: Login, title: '登录 - 零食仓库管理平台' },
  { path: '', component: Snacks, canActivate: [authGuard], title: '零食库存 - 零食仓库管理平台' },
  {
    path: 'reminders',
    component: Reminders,
    canActivate: [authGuard],
    title: '过期提醒 - 零食仓库管理平台',
  },
  {
    path: 'users',
    component: Users,
    canActivate: [authGuard, adminGuard],
    title: '账户管理 - 零食仓库管理平台',
  },
  {
    path: 'change-password',
    component: ChangePassword,
    canActivate: [authGuard],
    title: '修改密码 - 零食仓库管理平台',
  },
  { path: '**', redirectTo: '' },
];
