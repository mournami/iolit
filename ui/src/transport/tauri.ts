import { invoke } from '@tauri-apps/api/core';
import type { Transport } from './index';

export const tauriTransport: Transport = {
  call: (command, args) => invoke(command, args),
};
