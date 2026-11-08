import { invoke } from '@tauri-apps/api/core';
export const native=typeof window!=='undefined' && '__TAURI_INTERNALS__' in window;
export async function api<T>(command:string,args?:Record<string,unknown>):Promise<T>{
  if(!native) throw new Error('Откройте приложение Zapret Desktop. Браузерный просмотр не управляет системой.');
  return invoke<T>(command,args);
}
