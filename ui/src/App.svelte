<script lang="ts">
  import { onMount } from 'svelte';
  import { native } from './api';
  import Icon from './Icon.svelte';
  import { app, type Page } from './store.svelte';
  import { theme } from './theme.svelte';
  import { appearance } from './appearance.svelte';
  import { applyWindowIcon } from './windowIcon';
  import Checks from './components/Checks.svelte';
  import Mark from './components/Mark.svelte';
  import Overview from './components/Overview.svelte';
  import Power from './components/Power.svelte';
  import Settings from './components/Settings.svelte';
  import WindowControls from './components/WindowControls.svelte';

  // Windows draws its own title row (no system frame); other systems keep theirs.
  const ownFrame = native && navigator.userAgent.includes('Windows');

  const pages: { id: Page; label: string; icon: string }[] = [
    { id: 'overview', label: 'Обзор', icon: 'home' },
    { id: 'checks', label: 'Проверка', icon: 'activity' },
    { id: 'settings', label: 'Настройки', icon: 'settings' },
  ];
  const current = $derived(pages.find((p) => p.id === app.page) ?? pages[0]);

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

  // Taskbar and tray follow the icon chosen in settings. The exe icon is
  // already the default mark, so nothing is sent until the user picks one.
  let iconTouched = false;
  $effect(() => {
    const icon = appearance.icon;
    if (!native || (!icon && !iconTouched)) return;
    iconTouched = true;
    void applyWindowIcon(icon);
  });

  onMount(() => {
    void app.refresh();
    const timer = setInterval(() => void app.refresh(), 2000);
    return () => clearInterval(timer);
  });
</script>

<svelte:head><title>Iolit</title></svelte:head>

<div class="shell">
  <aside class="side" class:on={app.running}>
    <div class="brand" data-tauri-drag-region>
      <Mark size={24} /><span class="brand-word" data-tauri-drag-region>iolit</span>
    </div>
    <nav class="nav" aria-label="Разделы">
      {#each pages as p (p.id)}
        <button
          class:on={app.page === p.id}
          aria-current={app.page === p.id ? 'page' : undefined}
          onclick={() => (app.page = p.id)}
        >
          <Icon name={p.icon} size={18} />{p.label}
        </button>
      {/each}
    </nav>
    <div class="grow" data-tauri-drag-region></div>
    <Power />
  </aside>

  <div class="content">
    <header class="titlebar" data-tauri-drag-region>
      <p class="crumbs" data-tauri-drag-region><span>Iolit</span><span class="sep">/</span>{current.label}</p>
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

    <main class="main"><div class="page">
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
      {:else if app.page === 'checks'}
        <Checks />
      {:else}
        <Settings />
      {/if}
    </div></main>
  </div>
</div>

<style>
  .shell {
    height: 100vh;
    display: grid;
    grid-template-columns: 264px minmax(0, 1fr);
    padding: 8px 0 8px 8px;
    background: var(--bg);
  }

  /* The brand panel: a rounded column inside the window, one colour. */
  .side {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 16px 20px 24px;
    border-radius: 16px;
    background: var(--brand);
    color: var(--on-brand);
    --mark-bg: rgb(255 255 255 / 0.12);
    transition: background-color 0.3s;
    user-select: none;
  }
  .side.on {
    background: var(--brand-on);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 32px;
    padding-left: 4px;
  }
  .brand-word {
    font-size: 18px;
    font-weight: 500;
    line-height: 1;
  }
  .nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 24px -8px 0;
  }
  .nav button {
    height: 32px;
    padding: 0 8px;
    border-radius: 8px;
    border: 0;
    background: none;
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    font-size: 14px;
    font-weight: 500;
    color: var(--on-brand-2);
    transition: background-color 0.15s, color 0.15s;
  }
  .nav button:hover:not(.on) {
    color: var(--on-brand);
    background: rgb(255 255 255 / 0.06);
  }
  .nav button.on {
    background: rgb(255 255 255 / 0.12);
    color: var(--on-brand);
  }
  .nav button:focus-visible {
    outline-color: var(--on-brand);
    outline-offset: -2px;
    border-radius: 8px;
  }
  .grow {
    flex: 1;
  }

  .content {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .titlebar {
    height: 40px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px 0 24px;
    user-select: none;
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 14px;
    line-height: 20px;
    color: var(--text);
  }
  .crumbs span {
    color: var(--text-2);
  }
  .crumbs .sep {
    color: var(--text-3);
  }
  .svc-state {
    display: flex;
    align-items: center;
    gap: 8px;
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

  /* The page scrolls inside the window; the title bar stays. */
  .main {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 24px 48px 48px;
  }
  /* Like a document: one readable column, centred in wide windows. */
  .page {
    max-width: 880px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
  }

  /* Narrow windows: the brand panel becomes a band on top. */
  @media (max-width: 760px) {
    .shell {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto minmax(0, 1fr);
      padding: 8px 8px 0;
    }
    .side {
      padding: 16px 20px 20px;
    }
    .side .grow {
      display: none;
    }
    .nav {
      flex-direction: row;
      margin: 16px -12px 20px;
    }
    .titlebar {
      padding: 0 12px 0 20px;
    }
    .main {
      padding: 16px 20px 20px;
    }
    .crumbs {
      display: none;
    }
  }
</style>
