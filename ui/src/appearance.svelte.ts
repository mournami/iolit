/**
 * Colours, font and title-bar icon chosen by the user. Like the theme, stored
 * only in this window's local storage and never sent anywhere.
 */
import { accentTokens, isLight, normalizeHex, surfaceTokens } from './color';

export const FONTS = [
  { id: 'fixel', label: 'Fixel', stack: "'Fixel Text'", note: 'По умолчанию' },
  { id: 'system', label: 'Системный', stack: "'Segoe UI Variable Text', 'Segoe UI', system-ui", note: 'Как в Windows' },
  { id: 'bahnschrift', label: 'Bahnschrift', stack: 'Bahnschrift', note: 'Есть в Windows' },
  { id: 'cascadia', label: 'Cascadia Code', stack: "'Cascadia Code', 'Cascadia Mono'", note: 'Windows 11' },
  { id: 'trebuchet', label: 'Trebuchet MS', stack: "'Trebuchet MS'", note: 'Есть в Windows' },
  { id: 'verdana', label: 'Verdana', stack: 'Verdana', note: 'Крупный, читаемый' },
  { id: 'georgia', label: 'Georgia', stack: 'Georgia', note: 'С засечками' },
  { id: 'consolas', label: 'Consolas', stack: 'Consolas', note: 'Моноширинный' },
] as const;
export type FontId = (typeof FONTS)[number]['id'];

/** Default colours of the dark theme; the light theme keeps its own. */
export const DEFAULT_BG = '#000000';
export const DEFAULT_ACCENT = '#32216b';

/** The icon is stored as a square PNG of this size. */
export const ICON_SIZE = 256;

type Saved = { bg: string | null; accent: string | null; font: FontId; icon: string | null };
const KEY = 'zapret-appearance';
const TOKENS = [
  ...Object.keys(surfaceTokens(DEFAULT_BG)),
  ...Object.keys(accentTokens(DEFAULT_ACCENT, DEFAULT_BG)),
];

function load(): Saved {
  const empty: Saved = { bg: null, accent: null, font: 'fixel', icon: null };
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? '{}');
    return {
      bg: typeof raw.bg === 'string' ? normalizeHex(raw.bg) : null,
      accent: typeof raw.accent === 'string' ? normalizeHex(raw.accent) : null,
      font: FONTS.some((f) => f.id === raw.font) ? raw.font : 'fixel',
      icon: typeof raw.icon === 'string' && raw.icon.startsWith('data:image/png;base64,') ? raw.icon : null,
    };
  } catch {
    return empty;
  }
}

class Appearance {
  #saved = load();
  bg = $state<string | null>(this.#saved.bg);
  accent = $state<string | null>(this.#saved.accent);
  font = $state<FontId>(this.#saved.font);
  icon = $state<string | null>(this.#saved.icon);
  /** Set if the icon did not fit into local storage. */
  storageError = $state('');

  get custom() {
    return !!(this.bg || this.accent || this.font !== 'fixel' || this.icon);
  }

  /** Background the accent is shown on: the custom one or the theme's. */
  surface(theme: 'dark' | 'light') {
    return this.bg ?? (theme === 'light' ? '#f7f7f8' : DEFAULT_BG);
  }

  /** With a custom background the theme follows its brightness. */
  themeFor(fallback: 'dark' | 'light'): 'dark' | 'light' {
    return this.bg ? (isLight(this.bg) ? 'light' : 'dark') : fallback;
  }

  /** Writes CSS variables on <html>; called from the theme effect. */
  apply(theme: 'dark' | 'light') {
    const root = document.documentElement.style;
    for (const name of TOKENS) root.removeProperty(name);
    if (this.bg) for (const [k, v] of Object.entries(surfaceTokens(this.bg))) root.setProperty(k, v);
    if (this.accent)
      for (const [k, v] of Object.entries(accentTokens(this.accent, this.surface(theme)))) root.setProperty(k, v);
    const font = FONTS.find((f) => f.id === this.font) ?? FONTS[0];
    if (font.id === 'fixel') root.removeProperty('--sans');
    else root.setProperty('--sans', `${font.stack}, 'Fixel Text', 'Segoe UI', system-ui, sans-serif`);
  }

  setBg = (hex: string | null) => {
    this.bg = hex;
    this.#save();
  };
  setAccent = (hex: string | null) => {
    this.accent = hex;
    this.#save();
  };
  setFont = (font: FontId) => {
    this.font = font;
    this.#save();
  };
  setIcon = (dataUrl: string | null) => {
    const previous = this.icon;
    this.icon = dataUrl;
    if (!this.#save()) {
      this.icon = previous;
      this.storageError = 'Не удалось сохранить значок: в хранилище окна нет места.';
    }
  };
  reset = () => {
    this.bg = this.accent = this.icon = null;
    this.font = 'fixel';
    this.#save();
  };

  #save(): boolean {
    this.storageError = '';
    try {
      localStorage.setItem(KEY, JSON.stringify({ bg: this.bg, accent: this.accent, font: this.font, icon: this.icon }));
      return true;
    } catch {
      return false;
    }
  }
}

export const appearance = new Appearance();
