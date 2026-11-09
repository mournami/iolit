<script lang="ts">
  import { onMount } from 'svelte';
  import { native } from './api';
  import Icon from './Icon.svelte';
  import { app, type Page } from './store.svelte';
  import Checks from './components/Checks.svelte';
  import Mark from './components/Mark.svelte';
  import Overview from './components/Overview.svelte';
  import Settings from './components/Settings.svelte';

  const pages: { id: Page; label: string }[] = [
    { id: 'overview', label: 'Обзор' },
    { id: 'checks', label: 'Проверка' },
    { id: 'settings', label: 'Настройки' },
  ];

  const serviceState = $derived(
    !native
      ? { text: 'Только просмотр', tone: 'idle' }
      : !app.loaded
        ? { text: 'Подключаемся…', tone: 'idle' }
        : app.installed
          ? { text: 'Служба на связи', tone: 'ok' }
          : { text: 'Служба не найдена', tone: 'warn' },
  );

  onMount(() => {
    void app.refresh();
    const timer = setInterval(() => void app.refresh(), 2000);
    return () => clearInterval(timer);
  });
</script>

<svelte:head><title>Zapret Desktop</title></svelte:head>

<div class="shell">
  <header class="topbar">
    <div class="brand"><Mark size={22} /><span>zapret</span></div>
    <nav class="tabs" aria-label="Разделы">
      {#each pages as p (p.id)}
        <button class:active={app.page === p.id} aria-current={app.page === p.id ? 'page' : undefined} onclick={() => (app.page = p.id)}>
          {p.label}
        </button>
      {/each}
    </nav>
    <span class="service-state {serviceState.tone}"><span class="dot"></span>{serviceState.text}</span>
  </header>

  <main class="content">
    {#if app.error}
      <div class="alert fault" role="alert">
        <Icon name="alert" size={18} />
        <p>{app.error}</p>
        <button class="icon-btn" aria-label="Закрыть сообщение об ошибке" onclick={() => (app.error = '')}><Icon name="close" size={16} /></button>
      </div>
    {/if}
    {#if app.notice}
      <div class="alert signal" role="status">
        <Icon name="check" size={18} />
        <p>{app.notice}</p>
        <button class="icon-btn" aria-label="Закрыть уведомление" onclick={() => (app.notice = '')}><Icon name="close" size={16} /></button>
      </div>
    {/if}

    {#if app.page === 'overview'}
      <Overview />
    {:else if app.page === 'checks'}
      <Checks />
    {:else}
      <Settings />
    {/if}
  </main>
</div>
