<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { native } from './api';
  import Icon from './Icon.svelte';
  import { app, type Page } from './store.svelte';
  import { theme } from './theme.svelte';
  import { appearance } from './appearance.svelte';
  import { applyWindowIcon } from './windowIcon';
  import Mark from './components/Mark.svelte';
  import Overview from './components/Overview.svelte';
  import Settings from './components/Settings.svelte';
  import WindowControls from './components/WindowControls.svelte';

  // Windows draws its own title row (no system frame); other systems keep theirs.
  const ownFrame = native && navigator.userAgent.includes('Windows');

  const pages: { id: Page; label: string; icon: string }[] = [
    { id: 'overview', label: 'Главная', icon: 'home' },
    { id: 'settings', label: 'Настройки', icon: 'settings' },
  ];

  // The plate behind the active tab slides to the tab chosen.
  let tabs: HTMLElement | undefined = $state();
  let plate = $state({ x: 0, w: 0, ready: false });
  function movePlate() {
    const on = tabs?.querySelector<HTMLElement>('button.on');
    if (!tabs || !on) return;
    const a = on.getBoundingClientRect();
    const box = tabs.getBoundingClientRect();
    plate.x = a.x - box.x;
    plate.w = a.width;
  }
  $effect(() => {
    app.page;
    movePlate();
    // No slide on the first paint, only when the tab changes.
    requestAnimationFrame(() => (plate.ready = true));
  });

  // Taskbar and tray follow the icon chosen in settings, or the mark for the
  // theme: the exe icon is the light mark, so the dark theme swaps it.
  $effect(() => {
    const icon = appearance.icon;
    const resolved = theme.resolved;
    if (!native) return;
    void applyWindowIcon(icon, resolved);
  });

  onMount(() => {
    void app.refresh();
    const timer = setInterval(() => void app.refresh(), 2000);
    // Tab widths change once the font is loaded or the window is resized.
    void document.fonts?.ready.then(movePlate);
    window.addEventListener('resize', movePlate);
    return () => {
      clearInterval(timer);
      window.removeEventListener('resize', movePlate);
    };
  });
</script>

<svelte:head><title>Iolit</title></svelte:head>

<div class="shell">
  <header class="topbar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <Mark size={24} /><span class="brand-word" data-tauri-drag-region>iolit</span>
    </div>
    <nav class="tabs" aria-label="Разделы" bind:this={tabs}>
      <span
        class="plate"
        class:ready={plate.ready}
        style:width="{plate.w}px"
        style:transform="translateX({plate.x}px)"
        aria-hidden="true"
      ></span>
      {#each pages as p (p.id)}
        <button
          class:on={app.page === p.id}
          aria-current={app.page === p.id ? 'page' : undefined}
          onclick={() => (app.page = p.id)}
        >
          <Icon name={p.icon} size={16} />{p.label}
        </button>
      {/each}
    </nav>
    <div class="grow" data-tauri-drag-region></div>
    <button
      class="icon-btn"
      aria-label={theme.resolved === 'dark' ? 'Светлая тема' : 'Тёмная тема'}
      title={theme.resolved === 'dark' ? 'Светлая тема' : 'Тёмная тема'}
      onclick={theme.flip}
    >
      <Icon name={theme.resolved === 'dark' ? 'sun' : 'moon'} size={16} />
    </button>
    {#if ownFrame}<WindowControls />{/if}
  </header>

  <main class="main" class:edge={app.page === 'overview'}>
    <div class="page">
      {#if app.error}
        <div class="alert bad" role="alert">
          <Icon name="alert" size={16} />
          <p>{app.error}</p>
          <button class="icon-btn" aria-label="Закрыть сообщение об ошибке" onclick={() => (app.error = '')}>
            <Icon name="close" size={14} />
          </button>
        </div>
      {/if}
      {#if app.notice}
        <div class="alert good" role="status">
          <Icon name="check" size={16} />
          <p>{app.notice}</p>
          <button class="icon-btn" aria-label="Закрыть уведомление" onclick={() => (app.notice = '')}>
            <Icon name="close" size={14} />
          </button>
        </div>
      {/if}

      <!-- One short fade with a 6px rise; the old page is replaced at once.
           Only opacity and transform, so it stays smooth without GPU help. -->
      {#key app.page}
        <div class="view" in:fly={{ y: 6, duration: 160 }}>
          {#if app.page === 'overview'}
            <Overview />
          {:else}
            <Settings />
          {/if}
        </div>
      {/key}
    </div>
  </main>
</div>

<style>
  .shell {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }

  .topbar {
    height: 52px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 12px 0 20px;
    user-select: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-right: 24px;
  }
  .brand-word {
    font-family: var(--display);
    font-size: 18px;
    line-height: 24px;
    font-weight: 600;
  }
  .tabs {
    position: relative;
    display: flex;
    gap: 2px;
  }
  .plate {
    position: absolute;
    top: 0;
    left: 0;
    height: 32px;
    border-radius: 8px;
    background: var(--raise-2);
    pointer-events: none;
    opacity: 0;
  }
  .plate.ready {
    opacity: 1;
    transition: transform 0.26s cubic-bezier(0.3, 0.7, 0.2, 1), width 0.26s cubic-bezier(0.3, 0.7, 0.2, 1);
  }
  .tabs button {
    position: relative;
    height: 32px;
    padding: 0 12px 0 10px;
    border-radius: 8px;
    border: 0;
    background: none;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    font-weight: 500;
    color: var(--text-2);
    transition: background-color 0.15s, color 0.15s;
  }
  .tabs button:hover:not(.on) {
    color: var(--text);
    background: var(--hover);
  }
  .tabs button.on {
    color: var(--text);
  }
  .grow {
    flex: 1;
  }
  .view {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  /* The page scrolls inside the window; the top bar stays. */
  .main {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 16px 40px 48px;
  }
  /* Like a document: one readable column, centred in wide windows. */
  .page {
    max-width: 960px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
  }
  /* Home: the card sits at the left edge, under the logo; the content
     takes the rest of the width. */
  .main.edge {
    padding: 8px 20px 20px;
  }
  .main.edge .page {
    max-width: none;
    margin: 0;
  }

  @media (max-width: 760px) {
    .brand {
      margin-right: 8px;
    }
    .brand-word {
      display: none;
    }
    .main {
      padding: 12px 16px 24px;
    }
  }
</style>
