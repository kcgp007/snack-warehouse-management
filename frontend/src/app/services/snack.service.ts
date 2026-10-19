import { Injectable, inject } from '@angular/core';
import { HttpClient, HttpParams } from '@angular/common/http';
import { Observable } from 'rxjs';
import { Snack, SnackInput } from '../models';

export interface SnackQuery {
  /** name/notes 模糊搜索 */
  q?: string;
  /** 存放位置精确匹配 */
  location?: string;
  /** 过期维度：all|ok|expiring|expired（请求参数 expiry_status） */
  expiryStatus?: string;
  /** 生命周期维度：all|in_stock|opened|finished（请求参数 status） */
  status?: string;
}

/** /api/snacks 与 /api/locations（CONTRACT.md §3）。 */
@Injectable({ providedIn: 'root' })
export class SnackService {
  private readonly http = inject(HttpClient);

  list(query: SnackQuery): Observable<Snack[]> {
    let params = new HttpParams()
      .set('expiry_status', query.expiryStatus ? query.expiryStatus : 'all')
      .set('status', query.status ? query.status : 'all');
    if (query.q) {
      params = params.set('q', query.q);
    }
    if (query.location) {
      params = params.set('location', query.location);
    }
    return this.http.get<Snack[]>('/api/snacks', { params });
  }

  /** 已使用过的存放位置；传入 q 时按子串过滤（用于输入自动补全）。 */
  locations(q?: string): Observable<string[]> {
    let params = new HttpParams();
    const term = q?.trim();
    if (term) {
      params = params.set('q', term);
    }
    return this.http.get<string[]>('/api/locations', { params });
  }

  create(input: SnackInput): Observable<Snack> {
    return this.http.post<Snack>('/api/snacks', input);
  }

  update(id: number, input: SnackInput): Observable<Snack> {
    return this.http.put<Snack>(`/api/snacks/${id}`, input);
  }

  remove(id: number): Observable<void> {
    return this.http.delete<void>(`/api/snacks/${id}`);
  }
}
