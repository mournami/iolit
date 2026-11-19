<script lang="ts">
  import { untrack } from 'svelte';
  import { contrast, hexToHsv, hsvToHex, normalizeHex, type HSV } from '../color';
  import Icon from '../Icon.svelte';

  let {
    label,
    value,
    fallback,
    presets,
    against,
    onchange,
  }: {
    label: string;
    /** The chosen colour, or null for the theme's own. */
    value: string | null;
    /** Shown while `value` is null. */
    fallback: string;
    presets: string[];
    /** Warn when the colour nearly disappears on this one. */
    against?: string;
    onchange: (hex: string | null) => void;
  } = $props();

  const current = $derived(value ?? fallback);
  let open = $state(false);
  let hsv = $state<HSV>({ h: 0, s: 0, v: 0 });
  let hexText = $state('');
  let root: HTMLDivElement;
  const id = `cp-${Math.random().toString(36).slice(2, 8)}`;

  // Follow outside changes (reset, theme) without losing the hue of greys.
  // Only `current` is tracked, so typing a partial code is not overwritten.
  $effect(() => {
    const c = current;
    untrack(() => {
      if (hsvToHex(hsv) !== c) {
        const next = hexToHsv(c);
        hsv = next.s === 0 || next.v === 0 ? { ...next, h: hsv.h } : next;
      }
      if (normalizeHex(hexText) !== c) hexText = c.toUpperCase();
    });
  });

  const lowContrast = $derived(!!against && contrast(current, against) < 1.6);
  const eyeDropper = typeof window !== 'undefined' && 'EyeDropper' in window;

  function set(next: HSV) {
    hsv = next;
    onchange(hsvToHex(next));
  }

  function drag(e: PointerEvent, update: (x: number, y: number) => void) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect();
      update(Math.min(1, Math.max(0, (ev.clientX - r.left) / r.width)), Math.min(1, Math.max(0, (ev.clientY - r.top) / r.height)));
    };
    move(e);
    el.onpointermove = move;
    el.onpointerup = () => (el.onpointermove = el.onpointerup = null);
  }

  function areaKey(e: KeyboardEvent) {
    const step = e.shiftKey ? 0.1 : 0.02;
    const d: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, step], ArrowDown: [0, -step] };
    const m = d[e.key];
    if (!m) return;
    e.preventDefault();
    set({ ...hsv, s: Math.min(1, Math.max(0, hsv.s + m[0])), v: Math.min(1, Math.max(0, hsv.v + m[1])) });
  }

  function hueKey(e: KeyboardEvent) {
    const step = e.shiftKey ? 15 : 3;
    const m = e.key === 'ArrowLeft' || e.key === 'ArrowDown' ? -step : e.key === 'ArrowRight' || e.key === 'ArrowUp' ? step : 0;
    if (!m) return;
    e.preventDefault();
    set({ ...hsv, h: (hsv.h + m + 360) % 360 });
  }

  function typed() {
    const hex = normalizeHex(hexText);
    if (hex) onchange(hex);
  }

  async function pick() {
    try {
      // EyeDropper is in Chromium and WebView2; it picks from the whole screen.
      const result = await new (window as unknown as { EyeDropper: new () => { open(): Promise<{ sRGBHex: string }> } }).EyeDropper().open();
      const hex = normalizeHex(result.sRGBHex);
      if (hex) onchange(hex);
    } catch {
      // Cancelled with Escape.
    }
  }

  $effect(() => {
    if (!open) return;
    const outside = (e: PointerEvent) => {
      if (!root.contains(e.target as Node)) open = false;
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') open = false;
    };
    window.addEventListener('pointerdown', outside, true);
    window.addEventListener('keydown', key);
    return () => {
      window.removeEventListener('pointerdown', outside, true);
      window.removeEventListener('keydown', key);
    };
  });
</script>

<div class="cp" bind:this={root}>
  <button
    class="trigger"
    aria-haspopup="dialog"
    aria-expanded={open}
    aria-controls={id}
    aria-label={`${label}: ${current.toUpperCase()}`}
    onclick={() => (open = !open)}
  >
    <span class="chip" style:background={current}></span>
    <span class="mono">{current.toUpperCase()}</span>
    {#if !value}<span class="auto">тема</span>{/if}
    <Icon name="chevron" size={14} />
  </button>

  {#if open}
    <div class="pop" id={id} role="dialog" aria-label={label}>
      <div
        class="area"
        role="slider"
        tabindex="0"
        aria-label="Насыщенность и яркость"
        aria-valuetext={`насыщенность ${Math.round(hsv.s * 100)}%, яркость ${Math.round(hsv.v * 100)}%`}
        aria-valuenow={Math.round(hsv.v * 100)}
        style:background-color={hsvToHex({ h: hsv.h, s: 1, v: 1 })}
        onpointerdown={(e) => drag(e, (x, y) => set({ ...hsv, s: x, v: 1 - y }))}
        onkeydown={areaKey}
      >
        <span class="thumb" style:left={`${hsv.s * 100}%`} style:top={`${(1 - hsv.v) * 100}%`} style:background={current}></span>
      </div>

      <div
        class="hue"
        role="slider"
        tabindex="0"
        aria-label="Оттенок"
        aria-valuemin={0}
        aria-valuemax={360}
        aria-valuenow={Math.round(hsv.h)}
        onpointerdown={(e) => drag(e, (x) => set({ ...hsv, h: Math.min(359.9, x * 360) }))}
        onkeydown={hueKey}
      >
        <span class="thumb" style:left={`${(hsv.h / 360) * 100}%`} style:background={hsvToHex({ h: hsv.h, s: 1, v: 1 })}></span>
      </div>

      <div class="row">
        <span class="big-chip" style:background={current}></span>
        <label class="hex">
          <span class="sr-only">Код цвета</span>
          <span class="hash" aria-hidden="true">#</span>
          <input
            class="mono"
            value={hexText.replace(/^#/, '')}
            maxlength={7}
            spellcheck="false"
            autocomplete="off"
            oninput={(e) => {
              hexText = (e.currentTarget as HTMLInputElement).value;
              typed();
            }}
            onblur={() => (hexText = current.toUpperCase())}
          />
        </label>
        {#if eyeDropper}
          <button class="icon-btn" title="Взять цвет с экрана" aria-label="Взять цвет с экрана" onclick={pick}>
            <Icon name="pipette" size={16} />
          </button>
        {/if}
        <button class="icon-btn" title="Как в теме" aria-label="Вернуть цвет темы" disabled={!value} onclick={() => onchange(null)}>
          <Icon name="undo" size={16} />
        </button>
      </div>

      <div class="presets" role="group" aria-label="Готовые цвета">
        {#each presets as p (p)}
          <button
            class="preset"
            class:on={p === current}
            style:background={p}
            title={p.toUpperCase()}
            aria-label={p.toUpperCase()}
            onclick={() => onchange(p)}
          ></button>
        {/each}
      </div>

      {#if lowContrast}
        <p class="warn"><Icon name="alert" size={14} />Почти сливается с фоном, кнопки будет плохо видно</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .cp {
    position: relative;
  }
  .trigger {
    height: 34px;
    display: inline-flex;
    align-items: center;
    gap: 9px;
    padding: 0 10px 0 6px;
    border-radius: 9px;
    border: 1px solid var(--line-2);
    background: var(--raise);
    color: var(--text);
    font-size: 13px;
  }
  .trigger:hover {
    background: var(--raise-2);
  }
  .trigger :global(svg) {
    color: var(--text-2);
  }
  .chip {
    width: 22px;
    height: 22px;
    border-radius: 6px;
    box-shadow: inset 0 0 0 1px rgb(127 127 127 / 0.35);
  }
  .auto {
    font-size: 12px;
    color: var(--text-3);
  }

  .pop {
    position: absolute;
    right: 0;
    top: calc(100% + 8px);
    z-index: 30;
    width: 296px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    border-radius: 14px;
    background: var(--panel);
    border: 1px solid var(--line-2);
    box-shadow: 0 18px 50px rgb(0 0 0 / 0.45);
  }
  .area {
    position: relative;
    height: 176px;
    border-radius: 10px;
    background-image: linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, transparent);
    cursor: crosshair;
    touch-action: none;
  }
  .hue {
    position: relative;
    height: 14px;
    border-radius: 7px;
    background: linear-gradient(to right, #f00, #ff0 17%, #0f0 33%, #0ff 50%, #00f 67%, #f0f 83%, #f00);
    cursor: ew-resize;
    touch-action: none;
  }
  .thumb {
    position: absolute;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    transform: translate(-50%, -50%);
    border: 2.5px solid #fff;
    box-shadow: 0 0 0 1px rgb(0 0 0 / 0.35), 0 2px 6px rgb(0 0 0 / 0.35);
    pointer-events: none;
  }
  .hue .thumb {
    top: 50%;
  }
  .area:focus-visible,
  .hue:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .big-chip {
    width: 34px;
    height: 34px;
    flex: none;
    border-radius: 9px;
    box-shadow: inset 0 0 0 1px rgb(127 127 127 / 0.35);
  }
  .hex {
    flex: 1;
    position: relative;
    display: flex;
    align-items: center;
  }
  .hash {
    position: absolute;
    left: 11px;
    color: var(--text-3);
    font-family: var(--mono);
    font-size: 13px;
  }
  .hex input {
    width: 100%;
    padding-left: 24px;
    text-transform: uppercase;
  }

  .presets {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 6px;
  }
  .preset {
    aspect-ratio: 1;
    border: 0;
    border-radius: 7px;
    box-shadow: inset 0 0 0 1px rgb(127 127 127 / 0.3);
    transition: transform 0.12s;
  }
  .preset:hover {
    transform: scale(1.08);
  }
  .preset.on {
    box-shadow: 0 0 0 2px var(--panel), 0 0 0 4px var(--text);
  }
  .warn {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: var(--amber);
  }
</style>
