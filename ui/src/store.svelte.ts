import { save } from '@tauri-apps/plugin-dialog';
import { api, native, type Command } from './api';
import type { BundleStatus, DoctorCheck, DoctorResult, Snapshot } from './types';

export type Page = 'overview' | 'checks' | 'settings';

/** Command currently waiting for the service, used to word the busy state. */
type Pending = 'enable' | 'disable' | 'strategy' | 'install' | 'diagnose' | 'select' | 'cancel' | 'doctor' | 'dns' | 'update' | 'telegram' | null;

class AppState {
  page = $state<Page>('overview');
  data = $state<Snapshot | null>(null);
  /** True after the first snapshot request finished, successfully or not. */
  loaded = $state(false);
  error = $state('');
  notice = $state('');
  pending = $state<Pending>(null);
  domainBusy = $state(false);
  /** The user list as the service reports it, in readable form. */
  domains = $state<string[]>([]);
  /** Highlighted after adding, so it is clear what went into the list. */
  lastDomain = $state('');
  doctorChecks = $state<DoctorCheck[]>([]);
  doctorRan = $state(false);
  logLines = $state<string[]>([]);
  /** When this window saw the last check finish; the service does not report it. */
  checkedAt = $state<Date | null>(null);
  #refreshing = false;

  get service() {
    return this.data?.service;
  }
  get operation() {
    return this.data?.operation;
  }
  get bundle() {
    return this.data?.bundle;
  }
  get installed() {
    return !!this.data?.service.installed;
  }
  get running() {
    return !!this.data?.service.running;
  }
  get busy() {
    return this.pending !== null || !!this.data?.operation.busy;
  }

  refresh = async () => {
    if (this.#refreshing || !native) return;
    this.#refreshing = true;
    try {
      const wasBusy = !!this.data?.operation.busy;
      this.data = await api<Snapshot>('snapshot');
      if (wasBusy && !this.data.operation.busy && this.data.operation.results.length) this.checkedAt = new Date();
    } catch (e) {
      this.error = String(e);
    } finally {
      this.#refreshing = false;
      this.loaded = true;
    }
  };

  async #run(command: Command, args: Record<string, unknown> | undefined, pending: Pending) {
    this.pending = pending;
    this.error = '';
    this.notice = '';
    try {
      await api(command, args);
      await this.refresh();
      return true;
    } catch (e) {
      this.error = String(e);
      return false;
    } finally {
      this.pending = null;
    }
  }

  toggle = () => {
    const action = this.running ? 'disable' : 'enable';
    return this.#run(action, undefined, action);
  };
  setStrategy = (strategyId: string) => this.#run('set_strategy', { strategy_id: strategyId }, 'strategy');
  installService = () => this.#run('install_service', undefined, 'install');
  diagnose = () => this.#run('diagnose', undefined, 'diagnose');
  selectStrategy = () => {
    this.page = 'checks';
    return this.#run('select_strategy', undefined, 'select');
  };
  /** Selection only for the services the background check found broken. */
  repairStrategy = () => {
    this.page = 'checks';
    return this.#run('repair_strategy', undefined, 'select');
  };
  /** Runs the downloaded installer; the window closes and opens again. */
  updateApp = async () => {
    if (await this.#run('update_app', undefined, 'update'))
      this.notice = 'Устанавливаем обновление. Окно закроется и через минуту откроется снова.';
  };
  get appUpdate() {
    return this.data?.app;
  }
  get health() {
    return this.data?.health;
  }
  cancel = () => this.#run('cancel_operation', undefined, 'cancel');

  /** Secure DNS for the whole computer; on success the checks run again. */
  setSecureDns = async (enabled: boolean) => {
    if (!(await this.#run('set_secure_dns', { enabled }, 'dns'))) return;
    const provider = this.data?.service.secure_dns.provider ?? 'публичный DNS';
    this.notice = enabled
      ? `Защищённый DNS включён: ${provider} через HTTPS. Прежние настройки сохранены, их вернёт выключение в «Настройках» или удаление программы.`
      : 'Прежние настройки DNS возвращены.';
    if (!enabled) return;
    // Check again right away, keeping the notice: YouTube should turn green.
    try {
      await api('diagnose');
      await this.refresh();
    } catch (e) {
      this.error = String(e);
    }
  };

  get telegram() {
    return this.data?.telegram;
  }
  /** The local proxy for Telegram Desktop; it has its own switch. */
  setTelegram = async (enabled: boolean) => {
    if (!(await this.#run('set_telegram_proxy', { enabled }, 'telegram'))) return;
    this.notice = enabled
      ? 'Прокси для Telegram включён. Осталось подключить Telegram: нажмите «Открыть в Telegram».'
      : 'Прокси для Telegram выключен. Если Telegram перестал подключаться, отключите в нём этот прокси.';
  };
  /** Opens Telegram Desktop on its "add proxy" prompt. */
  openTelegram = async () => {
    this.error = '';
    this.notice = '';
    try {
      await api('open_telegram');
      this.notice = 'В Telegram должно открыться окно с прокси. Нажмите в нём «Подключить».';
    } catch (e) {
      this.error = String(e);
    }
  };
  copyTelegramLink = async () => {
    const link = this.data?.telegram.link;
    if (!link) return;
    try {
      await navigator.clipboard.writeText(link);
      this.notice = 'Ссылка скопирована. Отправьте её себе в «Избранное» в Telegram и нажмите на неё.';
    } catch (e) {
      this.error = `Не удалось скопировать ссылку: ${e}`;
    }
  };

  loadDomains = async () => {
    if (!native) return;
    try {
      this.domains = await api<string[]>('domains');
    } catch {
      // Without the service the panel keeps only the form.
    }
  };

  addDomain = async (domain: string) => {
    this.domainBusy = true;
    this.error = '';
    this.notice = '';
    try {
      await api('add_domain', { domain });
      await this.loadDomains();
      const typed = domain.trim().toLowerCase().replace(/\.$/, '');
      this.lastDomain = this.domains.find((d) => d === typed) ?? this.domains.at(-1) ?? '';
      this.notice = `Домен ${this.lastDomain || domain.trim()} добавлен. Если сайт уже был открыт, закройте его вкладку и откройте снова: браузер какое-то время держит старое соединение.`;
      return true;
    } catch (e) {
      this.error = String(e);
      return false;
    } finally {
      this.domainBusy = false;
    }
  };

  removeDomain = async (domain: string) => {
    this.domainBusy = true;
    this.error = '';
    this.notice = '';
    try {
      await api('remove_domain', { domain });
      await this.loadDomains();
      if (this.lastDomain === domain) this.lastDomain = '';
      this.notice = `Домен ${domain} удалён из вашего списка`;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.domainBusy = false;
    }
  };

  checkDoctor = async () => {
    this.pending = 'doctor';
    this.error = '';
    try {
      const r = await api<DoctorResult>('doctor');
      this.doctorChecks = r.checks || [];
      this.doctorRan = true;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.pending = null;
    }
  };

  loadLogs = async () => {
    if (!native) return;
    try {
      this.logLines = await api<string[]>('logs');
    } catch (e) {
      this.error = String(e);
    }
  };

  copyLogs = async () => {
    try {
      await navigator.clipboard.writeText(this.logLines.join('\n'));
      this.notice = 'Журнал скопирован. Перед тем как отправлять его кому-то, проверьте: в нём могут быть сетевые адреса.';
    } catch (e) {
      this.error = `Не удалось скопировать журнал: ${e}`;
    }
  };

  exportReport = async () => {
    try {
      const path = await save({
        defaultPath: 'zapret-diagnostics.json',
        filters: [{ name: 'Отчёт JSON', extensions: ['json'] }],
      });
      if (path) {
        await api('export_report', { path });
        this.notice = 'Отчёт сохранён. IP-адреса, ваши домены и журнал соединений в него не попали.';
      }
    } catch (e) {
      this.error = String(e);
    }
  };
}

export const app = new AppState();

export function deviceName(platform: string | undefined) {
  if (platform === 'windows') return 'Этот ПК';
  if (platform === 'macos') return 'Этот Mac';
  return 'Этот компьютер';
}

/** Russian plural: plural(5, 'домен', 'домена', 'доменов') → 'доменов'. */
export function plural(n: number, one: string, few: string, many: string) {
  const mod10 = n % 10;
  const mod100 = n % 100;
  if (mod10 === 1 && mod100 !== 11) return one;
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return few;
  return many;
}

export function formatUptime(secs: number) {
  if (secs < 60) return 'меньше минуты';
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h ? `${h} ч ${m} мин` : `${m} мин`;
}

/** One line about the strategy bundle in use, e.g. «Стратегии обновлены 2 ч назад». */
export function bundleLine(bundle: BundleStatus | undefined, now = Date.now() / 1000) {
  if (!bundle) return '';
  if (bundle.error) return 'Проблема с обновлением стратегий';
  if (bundle.version && bundle.last_update) return `Стратегии обновлены ${formatAgo(now - bundle.last_update)}`;
  return bundle.updates_enabled ? 'Стратегии из установщика' : 'Стратегии из установщика, обновления выключены';
}

function formatAgo(secs: number) {
  if (secs < 60) return 'только что';
  if (secs < 3600) return `${Math.floor(secs / 60)} мин назад`;
  if (secs < 86400) return `${Math.floor(secs / 3600)} ч назад`;
  return `${Math.floor(secs / 86400)} дн. назад`;
}

/** «23:21» for a check finished today. */
export const formatClock = (d: Date) => d.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' });

