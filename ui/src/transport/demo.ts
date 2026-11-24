// Development-only stand-in for the service, so the interface can be looked
// at in a browser with plausible data: `npm run dev`, then open `/?demo`.
// `?demo=off` starts with the bypass off, `?demo=new` like a fresh install (off,
// nothing chosen: the first enable also selects), `?demo=missing` without a service.
// Production builds never include it (see `index.ts`).
import type { ProbeResult, Snapshot } from '../types';
import type { Command, Transport } from './index';

const start = new URLSearchParams(location.search).get('demo') || 'on';
const now = () => Math.floor(Date.now() / 1000);

const probes: ProbeResult[] = [
  { id: 'internet', name: 'Интернет', protocol: 'tcp', ok: true, latency_ms: 18, detail: '' },
  { id: 'youtube', name: 'YouTube', protocol: 'tcp', ok: true, latency_ms: 84, detail: '' },
  { id: 'discord', name: 'Discord', protocol: 'tcp', ok: true, latency_ms: 61, detail: '' },
  { id: 'udp', name: 'UDP', protocol: 'udp', ok: false, latency_ms: 0, detail: 'STUN-сервер не ответил за 3 с' },
  { id: 'cloudflare', name: 'Cloudflare', protocol: 'tcp', ok: true, latency_ms: 47, detail: '' },
];

const state: Snapshot = {
  version: '0.3.1',
  service: {
    installed: start !== 'missing',
    running: start === 'on',
    platform: 'windows',
    engine: 'winws 72.6',
    strategy: start === 'missing' || start === 'new' ? '' : 'general-alt5',
    uptime_secs: 2 * 3600 + 17 * 60,
    capabilities: { tcp: true, udp: true, ipv6: false, packet_injection: true },
    warnings: [],
    packets: 0,
    modified_packets: 0,
    errors: 0,
    secure_dns: { supported: true, enabled: false },
  },
  strategies: [
    { id: 'general', name: 'general', description: 'Базовая стратегия Flowseal', supported: true, arguments: [] },
    { id: 'general-alt5', name: 'general (ALT5)', description: 'Вариант с подменой первых пакетов', supported: true, arguments: [] },
    { id: 'general-fake-tls', name: 'general (FAKE TLS AUTO)', description: 'Поддельный TLS', supported: true, arguments: [] },
  ],
  operation: { busy: false, stage: '', current: 0, total: 0, results: start === 'on' ? probes : [], error: null, winner: null, notice: null },
  bundle: { version: '2026.11.18', flowseal: '1.9.4', updates_enabled: true, last_check: now() - 1800, last_update: now() - 2 * 3600, error: null },
  health: { checked_at: now() - 240, broken: [] },
  app: { available: null, installing: false, last_check: now() - 3600, error: null },
  telegram: {
    enabled: false, running: false, link: null, active: 0, websocket: 0, direct: 0, failed: 0,
    failed_handshake: 0, failed_secret: 0, failed_route: 0, error: null,
  },
};

let domains = ['rutracker.org', 'nnmclub.to'];

/** Runs the checks one by one, like the service does. */
function runChecks(stage: string) {
  const op = state.operation;
  Object.assign(op, { busy: true, stage, current: 0, total: probes.length, results: [] });
  const step = () => {
    if (!op.busy) return;
    op.results = [...op.results, probes[op.current]];
    op.current += 1;
    if (op.current < op.total) setTimeout(step, 600);
    else Object.assign(op, { busy: false, stage: '' });
  };
  setTimeout(step, 600);
}

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const demoTransport: Transport = {
  async call<T>(command: Command, args?: Record<string, unknown>): Promise<T> {
    await delay(120);
    const result = (value: unknown) => structuredClone(value) as T;
    switch (command) {
      case 'snapshot':
        return result(state);
      case 'enable':
      case 'disable':
        await delay(500);
        state.service.running = command === 'enable';
        state.service.uptime_secs = 0;
        // Like the service: the first enable of a fresh install also selects.
        if (command === 'enable' && !state.service.strategy) {
          state.service.strategy = 'general-alt5';
          runChecks('Подбираем стратегию');
        }
        return result(null);
      case 'diagnose':
        runChecks('Проверяем сервисы');
        return result(null);
      case 'select_strategy':
      case 'repair_strategy':
        runChecks('Подбираем стратегию');
        return result(null);
      case 'cancel_operation':
        Object.assign(state.operation, { busy: false, stage: '' });
        return result(null);
      case 'set_strategy':
        state.service.strategy = String(args?.strategy_id ?? '');
        return result(null);
      case 'set_secure_dns':
        state.service.secure_dns = { supported: true, enabled: !!args?.enabled, provider: args?.enabled ? 'Cloudflare' : undefined };
        return result(null);
      case 'set_telegram_proxy':
        Object.assign(state.telegram, args?.enabled
          ? { enabled: true, running: true, link: 'tg://proxy?server=127.0.0.1&port=1443&secret=dd00', active: 3, websocket: 41, direct: 6, failed: 1, failed_route: 1 }
          : { enabled: false, running: false, link: null, active: 0 });
        return result(null);
      case 'doctor':
        return result({
          checks: [
            { name: 'Служба запущена', ok: true },
            { name: 'Движок winws работает', ok: state.service.running, detail: state.service.running ? undefined : 'Обход выключен' },
            { name: 'WinDivert не занят другой программой', ok: true },
            { name: 'GoodbyeDPI и старый zapret не запущены', ok: true },
            { name: 'DNS отдаёт настоящие адреса', ok: true },
          ],
        });
      case 'domains':
        return result(domains);
      case 'add_domain':
        domains = [...domains, String(args?.domain ?? '').trim().toLowerCase()];
        return result(null);
      case 'remove_domain':
        domains = domains.filter((d) => d !== args?.domain);
        return result(null);
      case 'logs':
        return result([
          'winws 72.6 started, strategy general-alt5',
          'windivert: filter loaded, 2 handles',
          'desync: fake,multisplit for 443/tcp',
          'desync: fake for 443/udp quic',
        ]);
      case 'install_service':
        state.service.installed = true;
        state.service.strategy = '';
        return result(null);
      case 'update_app':
      case 'export_report':
      case 'open_telegram':
        return result(null);
      default:
        throw new Error(`Демо не знает команду ${command}`);
    }
  },
};
