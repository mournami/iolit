<script lang="ts">
  import { onMount } from 'svelte';
  import { native } from './api';
  import Icon from './Icon.svelte';
  import { app, type Page } from './store.svelte';
  import { theme } from './theme.svelte';
  import { appearance } from './appearance.svelte';
  import { applyWindowIcon } from './windowIcon';
  import { layout } from './layout';
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

  const serviceState = $derived(
    !native
      ? { text: 'Только просмотр', tone: 'idle' }
      : !app.loaded
        ? { text: 'Подключаемся…', tone: 'idle' }
        : !app.installed
          ? { text: 'Служба не найдена', tone: 'warn' }
          : app.running
            ? { text: 'Служба работает', tone: 'ok' }
            : { text: 'Служба на связи', tone: 'ok' },
  );

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
    return () => clearInterval(timer);
  });
</script>

<svelte:head><title>Iolit</title></svelte:head>

<div class="shell">
  <header class="topbar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <Mark size={24} /><span class="brand-word" data-tauri-drag-region>iolit</span>
    </div>
    <nav class="tabs" aria-label="Разделы">
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
    <div class="svc-state {serviceState.tone}" title={serviceState.text}>
      <span class="dot"></span><span class="svc-text">{serviceState.text}</span>
    </div>
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

  <main class="main" class:edge={layout === 'b' && app.page === 'overview'}>
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

      {#if app.page === 'overview'}
        <Overview />
      {:else}
        <Settings />
      {/if}
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
    display: flex;
    gap: 2px;
  }
  .tabs button {
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
    background: var(--raise-2);
  }
  .grow {
    flex: 1;
  }
  .svc-state {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-right: 8px;
    font-size: 13px;
    color: var(--text-2);
    white-space: nowrap;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-3);
    flex: none;
  }
  .ok .dot {
    background: var(--ok);
  }
  .warn .dot {
    background: var(--amber);
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
  /* Layout B: the card sits at the left edge, under the logo; the content
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
    .brand-word,
    .svc-text {
      display: none;
    }
    .main {
      padding: 12px 16px 24px;
    }
  }
</style>
