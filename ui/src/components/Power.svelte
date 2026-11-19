<script lang="ts">
  // The bypass state and its switch. Lives in the brand side panel, so it is
  // visible from every page.
  import { native } from '../api';
  import Icon from '../Icon.svelte';
  import { app, formatUptime } from '../store.svelte';

  const service = $derived(app.service);
  const mode = $derived(
    !native ? 'preview' : !app.loaded ? 'loading' : !app.installed ? 'missing' : app.running ? 'on' : 'off',
  );

  const pendingText = $derived(
    app.pending === 'enable' ? 'Включаем обход…' : app.pending === 'disable' ? 'Выключаем обход…' : '',
  );

  const state = $derived.by(() => {
    switch (mode) {
      case 'preview':
        return {
          title: ['Только', 'просмотр'],
          sub: 'Здесь ничего не подключается',
          foot: 'Управлять сетью можно в установленном приложении Iolit.',
        };
      case 'loading':
        return { title: ['Связываемся', 'со службой'], sub: 'Это занимает пару секунд', foot: '' };
      case 'missing':
        return {
          title: ['Служба', 'не найдена'],
          sub: 'Без неё пакеты не изменяются',
          foot: 'Системную службу устанавливают один раз, понадобятся права администратора.',
        };
      case 'on':
        return {
          title: ['Обход', 'включён'],
          sub: service ? `Работает ${formatUptime(service.uptime_secs)}` : '',
          foot: 'Пакеты меняются только на этом компьютере. Трафик не уходит на наши серверы, телеметрии нет.',
        };
      default:
        return service?.strategy
          ? {
              title: ['Обход', 'выключен'],
              sub: 'Пакеты не изменяются',
              foot: 'Включите обход, чтобы приложение начало менять пакеты к заблокированным сервисам.',
            }
          : {
              title: ['Обход', 'выключен'],
              sub: 'Стратегия ещё не выбрана',
              foot: 'Подберите её автоматически: приложение проверит варианты и оставит тот, что работает в вашей сети.',
            };
    }
  });
</script>

<section class="power" class:on={mode === 'on'} aria-live="polite">
  <h1 class="title">{state.title[0]}<br />{state.title[1]}</h1>
  <p class="sub">{pendingText || state.sub}</p>

  {#if mode === 'on' || mode === 'off'}
    <button
      class="toggle"
      class:on={app.running}
      class:moving={!!pendingText}
      role="switch"
      aria-checked={app.running}
      aria-label="Обход блокировок"
      disabled={app.busy}
      onclick={app.toggle}
    >
      <span class="knob"></span>
    </button>
  {:else}
    <span class="badge"><Icon name={mode === 'missing' ? 'download' : 'power'} size={22} /></span>
  {/if}

  {#if mode === 'missing'}
    <div class="actions">
      <button class="btn light" disabled={app.busy} onclick={app.installService}>
        <Icon name="download" size={16} />{app.pending === 'install' ? 'Открываем установщик…' : 'Установить службу'}
      </button>
      <button class="btn quiet" disabled={app.busy} onclick={app.refresh}>Проверить снова</button>
    </div>
  {:else if mode === 'off' && !service?.strategy}
    <div class="actions">
      <button class="btn light" disabled={app.busy} onclick={app.selectStrategy}>
        <Icon name="wand" size={16} />Подобрать стратегию
      </button>
    </div>
  {/if}

  {#if state.foot}<p class="foot">{state.foot}</p>{/if}
</section>

<style>
  .power {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    min-width: 0;
    /* The title scales with the panel, so long words never overflow it. */
    container-type: inline-size;
  }
  .title {
    width: 100%;
    font-size: min(48px, 17cqi);
    line-height: 1;
    font-weight: 500;
    letter-spacing: -0.03em;
    color: var(--on-brand);
  }
  .sub {
    margin-top: 12px;
    font-size: 15px;
    color: var(--on-brand-2);
  }

  .toggle {
    position: relative;
    margin-top: 28px;
    width: 104px;
    height: 56px;
    padding: 0;
    border: 0;
    border-radius: 28px;
    background: rgb(255 255 255 / 0.12);
    transition: background-color 0.25s;
  }
  .knob {
    position: absolute;
    top: 6px;
    left: 6px;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    background: rgb(255 255 255 / 0.5);
    transition: transform 0.25s cubic-bezier(0.3, 0.7, 0.4, 1), background-color 0.25s;
  }
  .toggle:hover:not(:disabled) .knob {
    background: rgb(255 255 255 / 0.7);
  }
  .toggle.on {
    background: var(--ok);
  }
  .toggle.on .knob,
  .toggle.on:hover:not(:disabled) .knob {
    transform: translateX(48px);
    background: #ffffff;
  }
  .toggle.moving .knob {
    transform: translateX(24px);
  }
  .toggle:disabled:not(.moving) {
    opacity: 0.6;
  }
  .toggle:focus-visible {
    outline-color: var(--on-brand);
    outline-offset: 4px;
    border-radius: 28px;
  }
  .badge {
    margin-top: 28px;
    width: 56px;
    height: 56px;
    border-radius: 16px;
    display: grid;
    place-items: center;
    background: rgb(255 255 255 / 0.1);
    color: var(--on-brand);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 20px;
  }
  .btn.light {
    background: var(--on-brand);
    color: var(--brand);
  }
  .btn.light :global(svg) {
    color: var(--brand);
  }
  .btn.light:hover:not(:disabled) {
    background: #ffffff;
  }
  .btn.quiet {
    background: transparent;
    color: var(--on-brand-2);
  }
  .btn.quiet:hover:not(:disabled) {
    background: rgb(255 255 255 / 0.08);
    color: var(--on-brand);
  }

  .foot {
    margin-top: 24px;
    font-size: 13px;
    line-height: 1.5;
    color: var(--on-brand-2);
    max-width: 280px;
  }

  /* Narrow windows: the panel is a band on top, the switch goes right. */
  @media (max-width: 760px) {
    .power {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      grid-template-areas: 'title switch' 'sub switch' 'actions actions';
      align-items: center;
    }
    .title {
      grid-area: title;
      font-size: min(32px, 8cqi);
    }
    .sub {
      grid-area: sub;
      margin-top: 8px;
    }
    .toggle,
    .badge {
      grid-area: switch;
      margin-top: 0;
    }
    .actions {
      grid-area: actions;
    }
    .foot {
      display: none;
    }
  }
</style>
