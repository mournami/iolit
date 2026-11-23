/** Window theme. Stored only in this window's local storage, never sent anywhere. */
import { flushSync } from 'svelte';
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

/** Where the last click or tap landed: the colour spreads from there. */
let origin = { x: 0, y: 0, known: false };
if (typeof window !== 'undefined') {
  window.addEventListener(
    'pointerdown',
    (e) => {
      origin = { x: e.clientX, y: e.clientY, known: true };
    },
    true,
  );
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

  /**
   * Picking a theme drops a custom background, so the choice is visible. When
   * the colours really change, the new theme spreads over the window from the
   * click like ink. Only here, on a manual choice: when the system switches
   * between light and dark under "as in the system", the window changes at
   * once, with no animation and no extra load.
   */
  set = (choice: ThemeChoice) => {
    const next = choice === 'system' ? (this.#systemLight ? 'light' : 'dark') : choice;
    const change = () => {
      flushSync(() => {
        if (appearance.bg) appearance.setBg(null);
        this.choice = choice;
      });
    };
    const start = typeof document !== 'undefined' ? document.startViewTransition?.bind(document) : undefined;
    const calm = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (!start || calm || next === this.resolved) change();
    else {
      const x = origin.known ? origin.x : innerWidth - 40;
      const y = origin.known ? origin.y : 24;
      const radius = Math.hypot(Math.max(x, innerWidth - x), Math.max(y, innerHeight - y));
      const transition = start(change);
      void transition.ready.then(() =>
        document.documentElement.animate(
          { clipPath: [`circle(0px at ${x}px ${y}px)`, `circle(${radius}px at ${x}px ${y}px)`] },
          { duration: 650, easing: 'cubic-bezier(0.4, 0, 0.2, 1)', pseudoElement: '::view-transition-new(root)' },
        ),
      );
    }
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
