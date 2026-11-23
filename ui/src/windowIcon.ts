/**
 * Puts the user's icon, or the mark for the current theme, on the window
 * (taskbar, Alt+Tab) and the tray. The shortcuts use the light mark.
 */
import { TrayIcon } from '@tauri-apps/api/tray';
import { getCurrentWindow } from '@tauri-apps/api/window';
import lightIcon from '../../src-tauri/icons/128x128.png?inline';
import darkIcon from '../../src-tauri/icons/icon-dark.png?inline';
import type { MarkTheme } from './brandMark';

const icons = { light: lightIcon, dark: darkIcon };

function decode(dataUrl: string): Uint8Array {
  const binary = atob(dataUrl.slice(dataUrl.indexOf(',') + 1));
  return Uint8Array.from(binary, (c) => c.charCodeAt(0));
}

/** Rounds the corners like the in-app mark, so the taskbar matches. */
async function rounded(dataUrl: string): Promise<Uint8Array> {
  const img = new Image();
  img.src = dataUrl;
  await img.decode();
  const size = 256;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext('2d')!;
  ctx.beginPath();
  ctx.roundRect(0, 0, size, size, size * 0.3);
  ctx.clip();
  ctx.drawImage(img, 0, 0, size, size);
  return decode(canvas.toDataURL('image/png'));
}

/** The user's icon, or the built-in mark matching the theme. */
export async function applyWindowIcon(icon: string | null, theme: MarkTheme, signal: AbortSignal) {
  try {
    const bytes = icon ? await rounded(icon) : decode(icons[theme]);
    if (signal.aborted) return;
    const results = await Promise.allSettled([
      getCurrentWindow().setIcon(bytes),
      TrayIcon.getById('iolit').then((tray) => {
        if (!signal.aborted) return tray?.setIcon(bytes);
      }),
    ]);
    for (const result of results) {
      if (result.status === 'rejected') console.warn('Не удалось сменить значок Iolit', result.reason);
    }
  } catch (e) {
    console.warn('Не удалось подготовить значок Iolit', e);
  }
}
