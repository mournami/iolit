import { tauriTransport } from './tauri';
import type { DaemonCommand } from '../generated/api';

export type Command = DaemonCommand['cmd'] | 'install_service' | 'export_report';
export interface Transport {
  call<T>(command: Command, args?: Record<string, unknown>): Promise<T>;
}

export const native = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
const unavailable: Transport = {
  async call() {
    throw new Error('Откройте приложение Zapret Desktop. Браузерный просмотр не управляет системой.');
  },
};

// HTTP is deliberately not selected until there is an authenticated daemon server.
export const transport: Transport = native ? tauriTransport : unavailable;
