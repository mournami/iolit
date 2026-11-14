/** Window theme. Stored only in this window's local storage, never sent anywhere. */
import { appearance } from './appearance.svelte';

export type ThemeChoice = 'dark' | 'light' | 'system';

const KEY = 'zapret-theme';
const media = typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: light)') : null;

function stored(): ThemeChoice {
  try {
    const value = localStorage.getItem(KEY);
    if (value === 'dark' || value === 'light' || value === 'system') return value;
  } catch {
    // Storage can be unavailable; fall back to the default.
  }
  return 'dark';
}

class Theme {
  choice = $state<ThemeChoice>(stored());
  #systemLight = $state(!!media?.matches);

  constructor() {
    media?.addEventListener('change', (e) => (this.#systemLight = e.matches));
    $effect.root(() => {
      $effect(() => {
        const resolved = this.resolved;
        document.documentElement.dataset.theme = resolved;
        appearance.apply(resolved);
      });
    });
  }

  /** A custom background decides dark or light by its brightness. */
  get resolved(): 'dark' | 'light' {
    return appearance.themeFor(this.choice === 'system' ? (this.#systemLight ? 'light' : 'dark') : this.choice);
  }

  /** Picking a theme drops a custom background, so the choice is visible. */
  set = (choice: ThemeChoice) => {
    if (appearance.bg) appearance.setBg(null);
    this.choice = choice;
    try {
      localStorage.setItem(KEY, choice);
    } catch {
      // The choice still applies to this session.
    }
  };

  /** Title bar button: flip between dark and light. */
  flip = () => this.set(this.resolved === 'dark' ? 'light' : 'dark');
}

export const theme = new Theme();
