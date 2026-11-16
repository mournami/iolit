import type { ApiResponse } from '../generated/api';
import type { Transport } from './index';

/** Prepared for phase 3; no server or automatic HTTP selection in phase 0. */
export function createHttpTransport(baseUrl: string, token: string): Transport {
  if (!token) throw new Error('Нужен токен доступа к службе');
  const base = new URL(baseUrl);
  if (!['http:', 'https:'].includes(base.protocol)) throw new Error('Нужен HTTP-адрес службы');
  return {
    async call<T>(command: Parameters<Transport['call']>[0], args?: Record<string, unknown>): Promise<T> {
      if (command === 'install_service' || command === 'export_report' || command === 'open_telegram') {
        throw new Error('Эта команда доступна только в настольном приложении');
      }
      const response = await fetch(new URL(`/api/${command}`, base), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
        body: JSON.stringify(args ?? {}),
        credentials: 'omit',
        redirect: 'error',
      });
      if (!response.ok) throw new Error(`Служба ответила HTTP ${response.status}`);
      const result: ApiResponse = await response.json();
      if (!result.ok) throw new Error(result.error ?? 'Неизвестная ошибка службы');
      return result.data as T;
    },
  };
}
