<script lang="ts">
  // The bypass state and its switch on the brand colour: a wide banner above
  // the services, or a card beside them.
  import { native } from '../api';
  import Icon from '../Icon.svelte';
  import { app, formatUptime } from '../store.svelte';

  let { variant = 'banner' }: { variant?: 'banner' | 'card' } = $props();

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
          title: 'Только просмотр',
          sub: 'Здесь ничего не подключается',
          foot: 'Управлять сетью можно в установленном приложении Iolit.',
        };
      case 'loading':
        return { title: '', sub: 'Подключаемся к службе…', foot: '' };
      case 'missing':
        return {
          title: 'Служба не найдена',
          sub: 'Без неё пакеты не изменяются',
          foot: 'Системную службу устанавливают один раз, понадобятся права администратора.',
        };
      case 'on':
        return {
          title: 'Обход включён',
          sub: service ? `Работает ${formatUptime(service.uptime_secs)}` : '',
          foot: 'Пакеты меняются только на этом компьютере. Трафик не уходит на наши серверы, телеметрии нет.',
        };
      default:
        return service?.strategy
          ? {
              title: 'Обход выключен',
              sub: 'Пакеты не изменяются',
              foot: 'Включите обход, чтобы приложение начало менять пакеты к заблокированным сервисам.',
            }
          : {
              title: 'Обход выключен',
              sub: 'Стратегия ещё не выбрана',
              foot: 'Подберите её автоматически: приложение проверит варианты и оставит тот, что работает в вашей сети.',
            };
    }
  });
</script>

<section class="power {variant}" class:on={mode === 'on'} aria-live="polite">
  <div class="text">
    {#if mode === 'loading'}
      <!-- Connecting takes a second or two: one quiet line, not a headline. -->
      <p class="connecting"><span class="spin"><Icon name="refresh" size={16} /></span>{state.sub}</p>
    {:else}
      <h1 class="title">{state.title}</h1>
      <p class="sub">{pendingText || state.sub}</p>
    {/if}
  </div>

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
  {:else if mode !== 'loading'}
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
    min-width: 0;
    padding: 24px 28px;
    border-radius: 16px;
    background: var(--brand);
    color: var(--on-brand);
    transition: background-color 0.3s;
    /* The title scales with the block, so long words never overflow it. */
    container-type: inline-size;
  }
  .power.on {
    background: var(--brand-on);
  }

  /* Banner: text on the left, the switch on the right. */
  .banner {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    grid-template-areas: 'text switch' 'actions actions' 'foot foot';
    align-items: center;
    column-gap: 24px;
  }
  .banner .text {
    grid-area: text;
  }
  .banner .toggle,
  .banner .badge {
    grid-area: switch;
  }
  .banner .actions {
    grid-area: actions;
  }
  .banner .foot {
    grid-area: foot;
    max-width: 60ch;
    margin-top: 16px;
  }
  .banner .title {
    font-size: min(40px, 7cqi);
  }

  /* Card: everything stacked, the title at the top, the switch below. */
  .card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    padding: 28px 24px;
  }
  .card .title {
    max-width: 8ch;
    font-size: min(48px, 18cqi);
  }
  .card .toggle,
  .card .badge {
    margin-top: 28px;
  }
  .card .foot {
    margin-top: auto;
    padding-top: 24px;
  }

  .text {
    min-width: 0;
  }
  .title {
    font-family: var(--display);
    line-height: 1.05;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: var(--on-brand);
  }
  .sub {
    margin-top: 8px;
    font-size: 15px;
    line-height: 20px;
    color: var(--on-brand-2);
  }
  .connecting {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 15px;
    color: var(--on-brand-2);
  }
  .connecting span {
    display: flex;
  }

  .toggle {
    position: relative;
    flex: none;
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
    font-size: 13px;
    line-height: 20px;
    color: var(--on-brand-2);
  }

  @media (max-width: 640px) {
    .banner .foot {
      display: none;
    }
  }
</style>
