<script lang="ts">
  import { native } from '../api';
  import Icon from '../Icon.svelte';
  import { app, bundleLine, deviceName, formatCount, formatUptime } from '../store.svelte';
  import RouteMap from './RouteMap.svelte';

  const service = $derived(app.service);
  const op = $derived(app.operation);
  const results = $derived(op?.results ?? []);
  const mode = $derived(
    !native ? 'preview' : !app.loaded ? 'loading' : !app.installed ? 'missing' : app.running ? 'on' : 'off',
  );

  const copy = $derived.by(() => {
    switch (mode) {
      case 'preview':
        return {
          title: 'Только просмотр',
          text: 'Управлять сетью можно в установленном приложении Zapret Desktop. Здесь ничего не подключается и не имитируется.',
        };
      case 'loading':
        return { title: 'Связываемся со службой', text: 'Это занимает пару секунд.' };
      case 'missing':
        return {
          title: 'Служба не найдена',
          text: 'Без системной службы приложение не может менять сетевые пакеты. Её устанавливают один раз, понадобятся права администратора.',
        };
      case 'on':
        return {
          title: 'Обход включён',
          text: 'Пакеты к заблокированным сервисам изменяются прямо на этом компьютере. Проверьте, что нужные сервисы открываются.',
        };
      default:
        return service?.strategy
          ? { title: 'Обход выключен', text: 'Трафик идёт без изменений. Включите обход, чтобы приложение начало менять пакеты.' }
          : {
              title: 'Обход выключен',
              text: 'Стратегия ещё не выбрана. Подберите её автоматически: приложение проверит варианты и оставит тот, что работает в вашей сети.',
            };
    }
  });

  const pendingText = $derived(
    app.pending === 'enable' ? 'Включаем обход…' : app.pending === 'disable' ? 'Выключаем обход…' : '',
  );

  const checkedCount = $derived(results.filter((r) => r.id !== 'internet').length);
  const passedCount = $derived(results.filter((r) => r.id !== 'internet' && r.ok).length);
  const routeNote = $derived.by(() => {
    if (op?.busy) return op.total ? `${op.stage}, ${op.current} из ${op.total}` : op.stage;
    if (checkedCount) return `Прошли ${passedCount} из ${checkedCount} проверок.`;
    return 'Проверка покажет, какие соединения проходят в вашей сети.';
  });
</script>

<section class="hero" aria-live="polite">
  <div class="hero-text">
    <h1>{copy.title}</h1>
    <p>{pendingText || copy.text}</p>
    {#if mode === 'missing'}
      <div class="actions">
        <button class="btn btn-primary" disabled={app.busy} onclick={app.installService}>
          <Icon name="download" size={16} />{app.pending === 'install' ? 'Открываем установщик…' : 'Установить службу'}
        </button>
        <button class="btn btn-ghost" disabled={app.busy} onclick={app.refresh}>Проверить снова</button>
      </div>
    {:else if mode === 'off' && !service?.strategy}
      <div class="actions">
        <button class="btn btn-primary" disabled={app.busy} onclick={app.selectStrategy}>
          <Icon name="sliders" size={16} />Подобрать стратегию
        </button>
      </div>
    {/if}
  </div>

  {#if mode === 'on' || mode === 'off'}
    <button
      class="switch"
      class:on={app.running}
      class:moving={!!pendingText}
      role="switch"
      aria-checked={app.running}
      aria-label="Обход блокировок"
      disabled={app.busy}
      onclick={app.toggle}
    >
      <span class="thumb"></span>
    </button>
  {/if}
</section>

<section class="panel route-panel" aria-labelledby="route-title">
  <div class="panel-head">
    <div>
      <h2 id="route-title">Доступ к сервисам</h2>
      <p aria-live="polite">{routeNote}</p>
    </div>
    <button class="btn btn-secondary" disabled={!native || app.busy} onclick={app.diagnose}>
      <Icon name="refresh" size={16} />{op?.busy ? 'Проверяем…' : 'Проверить'}
    </button>
  </div>
  {#if op?.busy}
    <div class="progress" class:indeterminate={!op.total}>
      <span style:width={op.total ? `${(op.current / op.total) * 100}%` : null}></span>
    </div>
  {/if}
  <RouteMap
    running={app.running}
    {results}
    checking={!!op?.busy}
    deviceLabel={deviceName(service?.platform)}
    deviceIcon={service?.platform === 'macos' ? 'laptop' : 'monitor'}
  />
</section>

<dl class="facts">
  <div>
    <dt>Стратегия</dt>
    <dd>
      <span class="value">{service?.strategy || (app.installed ? 'Не выбрана' : '—')}</span>
      {#if app.installed && service?.strategy}
        <button class="link" disabled={app.busy} onclick={app.selectStrategy}>Подобрать заново</button>
      {/if}
      {#if app.installed && app.bundle}
        <span class="sub" title={app.bundle.error ?? (app.bundle.flowseal ? `Flowseal ${app.bundle.flowseal}` : undefined)}>
          {bundleLine(app.bundle)}
        </span>
      {/if}
    </dd>
  </div>
  <div>
    <dt>Движок</dt>
    <dd><span class="value">{service?.engine || '—'}</span></dd>
  </div>
  <div>
    <dt>Работает</dt>
    <dd><span class="value">{app.running && service ? formatUptime(service.uptime_secs) : '—'}</span></dd>
  </div>
  <div>
    <dt>Изменено пакетов</dt>
    <dd>
      <span class="value num">{app.running && service ? formatCount(service.modified_packets) : '—'}</span>
      {#if app.running && service?.packets}
        <span class="sub num">из {formatCount(service.packets)}</span>
      {/if}
    </dd>
  </div>
</dl>

{#if service?.warnings.length}
  <ul class="warnings">
    {#each service.warnings as warning}
      <li><Icon name="alert" size={16} /><span>{warning}</span></li>
    {/each}
  </ul>
{/if}

<p class="footnote">Обход работает на этом компьютере. Трафик не уходит на наши серверы, телеметрия не отправляется.</p>

<style>
  .hero {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 32px;
    margin-bottom: 28px;
  }
  .hero-text {
    max-width: 560px;
  }
  .hero h1 {
    font-family: var(--display);
    font-size: 36px;
    line-height: 1.1;
    font-weight: 650;
    letter-spacing: -0.025em;
  }
  .hero p {
    margin-top: 10px;
    color: var(--ink-2);
    font-size: 15px;
    line-height: 1.5;
    max-width: 62ch;
  }
  .actions {
    display: flex;
    gap: 10px;
    margin-top: 20px;
    flex-wrap: wrap;
  }

  /* The on/off control */
  .switch {
    position: relative;
    flex: none;
    width: 84px;
    height: 46px;
    margin-top: 2px;
    padding: 0;
    border: 1px solid var(--line-strong);
    border-radius: 999px;
    background: var(--raised);
    transition: background-color 0.2s, border-color 0.2s;
  }
  .switch .thumb {
    position: absolute;
    top: 4px;
    left: 4px;
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: var(--ink-2);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.35);
    transition: transform 0.22s cubic-bezier(0.3, 0.7, 0.4, 1), background-color 0.2s;
  }
  .switch:hover:not(:disabled) .thumb {
    background: var(--ink);
  }
  .switch.on {
    background: var(--signal);
    border-color: var(--signal);
  }
  .switch.on .thumb {
    transform: translateX(38px);
    background: var(--bg);
  }
  .switch.moving .thumb {
    transform: translateX(19px);
  }
  .switch:disabled {
    opacity: 1;
  }
  .switch:disabled:not(.moving) {
    opacity: 0.6;
  }

  .route-panel {
    position: relative;
    overflow: hidden;
  }
  .route-panel .progress {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    border-radius: 0;
  }

  /* Facts under the map */
  .facts {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 24px;
    margin: 24px 0 0;
    padding: 0 4px;
  }
  .facts dt {
    font-size: 12px;
    color: var(--ink-3);
  }
  .facts dd {
    margin: 6px 0 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    min-width: 0;
  }
  .facts .value {
    font-size: 15px;
    font-weight: 560;
    color: var(--ink);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .facts .sub {
    font-size: 12px;
    color: var(--ink-3);
  }

  .warnings {
    list-style: none;
    margin: 24px 0 0;
    padding: 14px 16px;
    border: 1px solid var(--caution-line);
    background: var(--caution-wash);
    border-radius: 10px;
    color: var(--caution-ink);
    display: grid;
    gap: 8px;
  }
  .warnings li {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    line-height: 1.45;
  }
  .warnings :global(svg) {
    margin-top: 1px;
  }

  .footnote {
    margin-top: 28px;
    padding: 0 4px;
    font-size: 12px;
    color: var(--ink-3);
  }

  @media (max-width: 860px) {
    .hero h1 {
      font-size: 30px;
    }
  }
  @media (max-width: 700px) {
    .facts {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      row-gap: 18px;
    }
  }
</style>
