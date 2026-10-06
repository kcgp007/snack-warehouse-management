import { apiErrorMessage } from './api-error';

describe('apiErrorMessage', () => {
  it('reads the contract error body {"error": "..."}', () => {
    expect(apiErrorMessage({ error: { error: '不能删除当前登录账户' } }, '默认')).toBe(
      '不能删除当前登录账户',
    );
  });

  it('reads a plain-text error payload', () => {
    expect(apiErrorMessage({ error: '用户名已存在' }, '默认')).toBe('用户名已存在');
  });

  it('falls back when the payload has no message', () => {
    expect(apiErrorMessage({ error: {} }, '默认')).toBe('默认');
    expect(apiErrorMessage(null, '默认')).toBe('默认');
  });
});
