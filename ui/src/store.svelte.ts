import { save } from '@tauri-apps/plugin-dialog';
import { api, native } from './api';
import type { DoctorCheck, Snapshot } from './types';

export type Page = 'overview' | 'checks' | 'settings';

/** Command currently waiting for the service, used to word the busy state. */
type Pending = 'enable' | 'disable' | 'strategy' | 'install' | 'diagnose' | 'select' | 'cancel' | 'doctor' | null;

class AppState {
  page = $state<Page>('overview');
  data = $state<Snapshot | null>(null);
  /** True after the first snapshot request finished, successfully or not. */
  loaded = $state(false);
  error = $state('');
  notice = $state('');
  pending = $state<Pending>(null);
  domainBusy = $state(false);
  doctorChecks = $state<DoctorCheck[]>([]);
  doctorRan = $state(false);
  logLines = $state<string[]>([]);
  showLogs = $state(false);
  #refreshing = false;

  get service() {
    return this.data?.service;
  }
  get operation() {
    return this.data?.operation;
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
      this.data = await api<Snapshot>('snapshot');
    } catch (e) {
      this.error = String(e);
    } finally {
      this.#refreshing = false;
      this.loaded = true;
    }
  };

  async #run(command: string, args: Record<string, unknown> | undefined, pending: Pending) {
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
    return this.#run('control', { action }, action);
  };
  setStrategy = (strategyId: string) => this.#run('control', { action: 'strategy', strategyId }, 'strategy');
  installService = () => this.#run('install_service', undefined, 'install');
  diagnose = () => this.#run('diagnose', undefined, 'diagnose');
  selectStrategy = () => {
    this.page = 'checks';
    return this.#run('select_strategy', undefined, 'select');
  };
  cancel = () => this.#run('cancel_operation', undefined, 'cancel');

  addDomain = async (domain: string) => {
    this.domainBusy = true;
    this.error = '';
    this.notice = '';
    try {
      await api('add_domain', { domain });
      this.notice = `Домен ${domain.trim()} добавлен в ваш список`;
      return true;
    } catch (e) {
      this.error = String(e);
      return false;
    } finally {
      this.domainBusy = false;
    }
  };

  checkDoctor = async () => {
    this.pending = 'doctor';
    this.error = '';
    try {
      const r = await api<{ checks?: DoctorCheck[] }>('doctor');
      this.doctorChecks = r.checks || [];
      this.doctorRan = true;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.pending = null;
    }
  };

  toggleLogs = async () => {
    this.showLogs = !this.showLogs;
    if (!this.showLogs) return;
    try {
      this.logLines = await api<string[]>('logs');
    } catch (e) {
      this.error = String(e);
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

export function formatUptime(secs: number) {
  if (secs < 60) return 'меньше минуты';
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h ? `${h} ч ${m} мин` : `${m} мин`;
}

export const formatCount = (n: number) => n.toLocaleString('ru-RU');
