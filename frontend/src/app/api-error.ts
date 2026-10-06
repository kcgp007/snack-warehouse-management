/**
 * 从 HttpClient 错误中提取契约统一错误体 `{"error": "人类可读信息"}` 的文案。
 * 兼容后端返回 JSON 对象或纯文本两种情况。
 */
export function apiErrorMessage(err: unknown, fallback: string): string {
  if (err && typeof err === 'object' && 'error' in err) {
    const payload = (err as { error: unknown }).error;
    if (typeof payload === 'string' && payload.trim() !== '') {
      return payload;
    }
    if (payload && typeof payload === 'object' && 'error' in payload) {
      const nested = (payload as { error: unknown }).error;
      if (typeof nested === 'string' && nested.trim() !== '') {
        return nested;
      }
    }
  }
  return fallback;
}
