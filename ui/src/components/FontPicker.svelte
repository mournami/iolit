<script lang="ts">
  import { FONTS, type FontId } from '../appearance.svelte';
  import Icon from '../Icon.svelte';

  let { value, onchange }: { value: FontId; onchange: (font: FontId) => void } = $props();

  let open = $state(false);
  let active = $state(0);
  let root: HTMLDivElement;
  let list = $state<HTMLUListElement>();
  const current = $derived(FONTS.find((f) => f.id === value) ?? FONTS[0]);
  const family = (stack: string) => `${stack}, 'Inter Variable', sans-serif`;

  function toggle() {
    open = !open;
    active = Math.max(0, FONTS.findIndex((f) => f.id === value));
    if (open) queueMicrotask(() => list?.focus());
  }

  function choose(i: number) {
    onchange(FONTS[i].id);
    open = false;
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') active = (active + 1) % FONTS.length;
    else if (e.key === 'ArrowUp') active = (active - 1 + FONTS.length) % FONTS.length;
    else if (e.key === 'Enter' || e.key === ' ') choose(active);
    else if (e.key === 'Escape') open = false;
    else return;
    e.preventDefault();
    list?.querySelector(`[data-i="${active}"]`)?.scrollIntoView({ block: 'nearest' });
  }

  $effect(() => {
    if (!open) return;
    const outside = (e: PointerEvent) => {
      if (!root.contains(e.target as Node)) open = false;
    };
    window.addEventListener('pointerdown', outside, true);
    return () => window.removeEventListener('pointerdown', outside, true);
  });
</script>

<div class="fp" bind:this={root}>
  <button class="trigger" aria-haspopup="listbox" aria-expanded={open} onclick={toggle}>
    <span style:font-family={family(current.stack)}>{current.label}</span>
    <Icon name="chevron" size={14} />
  </button>
  {#if open}
    <ul
      class="pop"
      role="listbox"
      tabindex="-1"
      aria-label="Шрифт"
      aria-activedescendant={`font-${FONTS[active].id}`}
      bind:this={list}
      onkeydown={key}
    >
      {#each FONTS as f, i (f.id)}
        <li
          id={`font-${f.id}`}
          data-i={i}
          role="option"
          aria-selected={f.id === value}
          class:active={i === active}
          onpointerenter={() => (active = i)}
          onclick={() => choose(i)}
          onkeydown={() => {}}
        >
          <span class="name" style:font-family={family(f.stack)}>{f.label}</span>
          <span class="sample" style:font-family={family(f.stack)}>Обход включён · 0123</span>
          <span class="note">{f.note}</span>
          {#if f.id === value}<span class="tick"><Icon name="check" size={15} stroke={2.6} /></span>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .fp {
    position: relative;
  }
  .trigger {
    height: 34px;
    min-width: 168px;
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 0 10px 0 12px;
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
  .pop {
    position: absolute;
    right: 0;
    top: calc(100% + 8px);
    z-index: 30;
    width: 300px;
    max-height: 340px;
    overflow: auto;
    padding: 6px;
    list-style: none;
    border-radius: 14px;
    background: var(--panel);
    border: 1px solid var(--line-2);
    box-shadow: 0 18px 50px rgb(0 0 0 / 0.45);
    outline: none;
  }
  li {
    position: relative;
    display: grid;
    grid-template-columns: 1fr auto;
    grid-template-areas: 'name note' 'sample sample';
    gap: 2px 10px;
    padding: 9px 36px 9px 12px;
    border-radius: 9px;
    cursor: pointer;
  }
  li.active {
    background: var(--hover);
  }
  li[aria-selected='true'] {
    background: var(--g-soft);
  }
  .name {
    grid-area: name;
    font-size: 15px;
    font-weight: 500;
  }
  .note {
    grid-area: note;
    align-self: center;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .sample {
    grid-area: sample;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .tick {
    position: absolute;
    right: 12px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--g-text);
    display: flex;
  }
</style>
