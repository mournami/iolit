import { tauriTransport } from './tauri';
import type { DaemonCommand } from '../generated/api';

export type Command = DaemonCommand['cmd'] | 'install_service' | 'export_report' | 'open_telegram';
export interface Transport {
  call<T>(command: Command, args?: Record<string, unknown>): Promise<T>;
}

const tauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
/** `npm run dev` with `?demo`: plausible data instead of the service. Never in a build. */
const demo = import.meta.env.DEV && typeof location !== 'undefined' && new URLSearchParams(location.search).has('demo');
export const native = tauri || demo;
const unavailable: Transport = {
  async call() {
    throw new Error('Откройте приложение Iolit. Браузерный просмотр не управляет системой.');
  },
};
const demoTransport: Transport = {
  async call(command, args) {
    return (await import('./demo')).demoTransport.call(command, args);
  },
};

// HTTP is deliberately not selected until there is an authenticated daemon server.
export const transport: Transport = tauri ? tauriTransport : demo ? demoTransport : unavailable;
