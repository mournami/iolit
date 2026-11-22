<script lang="ts">
  // The bypass state and its switch: a wide banner above the services, in
  // the brand colour while the bypass is on.
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

<section class="power" class:on={mode === 'on'} aria-live="polite">
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
  /* Off: a quiet dark slab. On: the whole block in the brand colour. */
  .power {
    min-width: 0;
    padding: 24px 28px;
    border-radius: 20px;
    background: var(--slab-off);
    color: var(--text);
    transition: background-color 0.25s, color 0.25s;
    /* The title scales with the block, so long words never overflow it. */
    container-type: inline-size;
  }
  .power.on {
    background: var(--brand);
    color: var(--on-brand);
  }

  /* Text on the left, the switch on the right. */
  .power {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    grid-template-areas: 'text switch' 'actions actions' 'foot foot';
    align-items: center;
    column-gap: 24px;
  }
  .power .text {
    grid-area: text;
  }
  .power .toggle,
  .power .badge {
    grid-area: switch;
  }
  .power .actions {
    grid-area: actions;
  }
  .power .foot {
    grid-area: foot;
    max-width: 60ch;
    margin-top: 16px;
  }
  .power .title {
    font-size: min(40px, 7cqi);
  }

  .text {
    min-width: 0;
  }
  .title {
    font-family: var(--display);
    line-height: 1;
    font-weight: 600;
    letter-spacing: -0.03em;
  }
  .sub {
    margin-top: 16px;
    font-size: 15px;
    line-height: 20px;
    font-weight: 500;
    opacity: 0.72;
  }
  .connecting {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 15px;
    opacity: 0.72;
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
    background: var(--line-2);
    transition: background-color 0.25s;
  }
  .knob {
    position: absolute;
    top: 6px;
    left: 6px;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    background: var(--text-3);
    transition: transform 0.25s cubic-bezier(0.3, 0.7, 0.4, 1), background-color 0.25s;
  }
  .toggle:hover:not(:disabled) .knob {
    background: var(--text-2);
  }
  /* On: a dark track cut into the banner and a light knob. */
  .toggle.on {
    background: #0b0718;
  }
  .toggle.on .knob,
  .toggle.on:hover:not(:disabled) .knob {
    transform: translateX(48px);
    background: var(--on-brand);
  }
  .toggle.moving .knob {
    transform: translateX(24px);
  }
  .toggle:disabled:not(.moving) {
    opacity: 0.6;
  }
  .toggle:focus-visible {
    outline-color: currentColor;
    outline-offset: 4px;
    border-radius: 28px;
  }
  .badge {
    width: 56px;
    height: 56px;
    border-radius: 16px;
    display: grid;
    place-items: center;
    background: var(--raise-2);
    color: var(--text-2);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 20px;
  }
  .btn.light {
    background: var(--brand);
    color: var(--on-brand);
  }
  .btn.light :global(svg) {
    color: var(--on-brand);
  }
  .btn.light:hover:not(:disabled) {
    background: color-mix(in srgb, var(--brand) 82%, #ffffff);
  }
  .btn.quiet {
    background: transparent;
    color: var(--text-2);
  }
  .btn.quiet:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }

  .foot {
    font-size: 13px;
    line-height: 20px;
    opacity: 0.66;
  }

  @media (max-width: 640px) {
    .power .foot {
      display: none;
    }
  }
</style>
