import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { provideRouter } from '@angular/router';
import { authInterceptor } from './auth.interceptor';
import { AuthService } from '../services/auth.service';

@Component({ template: 'ok' })
class StubPage {}

const TOKEN_KEY = 'snack-warehouse.token';

describe('authInterceptor', () => {
  let httpMock: HttpTestingController;

  beforeEach(() => {
    localStorage.removeItem(TOKEN_KEY);
    TestBed.configureTestingModule({
      providers: [
        provideRouter([
          { path: 'login', component: StubPage },
          { path: '', component: StubPage },
          { path: '**', redirectTo: '' },
        ]),
        provideHttpClient(withInterceptors([authInterceptor])),
        provideHttpClientTesting(),
      ],
    });
    httpMock = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    httpMock.verify();
    localStorage.removeItem(TOKEN_KEY);
  });

  it('attaches Authorization: Bearer <token> from localStorage', () => {
    localStorage.setItem(TOKEN_KEY, 'token-123');
    const auth = TestBed.inject(AuthService);
    TestBed.inject(HttpClient).get('/api/snacks').subscribe({ error: () => undefined });

    const req = httpMock.expectOne('/api/snacks');
    expect(req.request.headers.get('Authorization')).toBe('Bearer token-123');
    req.flush([]);
    expect(auth.token()).toBe('token-123');
  });

  it('does not attach a token to the login request', () => {
    localStorage.setItem(TOKEN_KEY, 'token-123');
    TestBed.inject(AuthService);
    TestBed.inject(HttpClient).post('/api/auth/login', {}).subscribe({ error: () => undefined });

    const req = httpMock.expectOne('/api/auth/login');
    expect(req.request.headers.get('Authorization')).toBeNull();
    req.flush({ token: 'x', user: { id: 1, username: 'u', is_admin: false } });
  });

  it('clears the stored session when any API call returns 401', () => {
    localStorage.setItem(TOKEN_KEY, 'expired-token');
    const auth = TestBed.inject(AuthService);
    TestBed.inject(HttpClient).get('/api/snacks').subscribe({ error: () => undefined });

    const req = httpMock.expectOne('/api/snacks');
    req.flush({ error: '未授权' }, { status: 401, statusText: 'Unauthorized' });
    expect(auth.token()).toBeNull();
    expect(auth.user()).toBeNull();
    expect(localStorage.getItem(TOKEN_KEY)).toBeNull();
  });
});
