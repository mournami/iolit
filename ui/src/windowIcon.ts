/**
 * Puts the user's icon, or the mark for the current theme, on the window
 * (taskbar, Alt+Tab) and the tray. The Start menu and desktop shortcuts keep
 * the exe's icon, the light mark.
 */
import { TrayIcon } from '@tauri-apps/api/tray';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { markSvg, type MarkTheme } from './brandMark';

function decode(dataUrl: string): Uint8Array {
  const binary = atob(dataUrl.slice(dataUrl.indexOf(',') + 1));
  return Uint8Array.from(binary, (c) => c.charCodeAt(0));
}

async function renderDefault(theme: MarkTheme): Promise<Uint8Array> {
  const img = new Image();
  img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(markSvg(theme, 256));
  await img.decode();
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 256;
  canvas.getContext('2d')!.drawImage(img, 0, 0, 256, 256);
  return decode(canvas.toDataURL('image/png'));
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
export async function applyWindowIcon(icon: string | null, theme: MarkTheme) {
  try {
    const bytes = icon ? await rounded(icon) : await renderDefault(theme);
    await getCurrentWindow().setIcon(bytes);
    const tray = await TrayIcon.getById('main');
    await tray?.setIcon(bytes);
  } catch (e) {
    // The in-app mark still changes; only the taskbar keeps the old icon.
    console.warn('Не удалось сменить значок окна', e);
  }
}
