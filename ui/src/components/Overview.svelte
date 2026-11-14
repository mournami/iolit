<script lang="ts">
  import { native } from '../api';
  import Icon from '../Icon.svelte';
  import { app, bundleLine, formatClock, formatUptime, plural } from '../store.svelte';
  import type { ProbeResult } from '../types';

  const service = $derived(app.service);
  const activeStrategy = $derived(app.data?.strategies.find((s) => s.id === service?.strategy));
  const op = $derived(app.operation);
  const results = $derived(op?.results ?? []);
  const mode = $derived(
    !native ? 'preview' : !app.loaded ? 'loading' : !app.installed ? 'missing' : app.running ? 'on' : 'off',
  );

  const pendingText = $derived(
    app.pending === 'enable' ? 'Включаем обход…' : app.pending === 'disable' ? 'Выключаем обход…' : '',
  );

  const slab = $derived.by(() => {
    switch (mode) {
      case 'preview':
        return {
          title: ['Только', 'просмотр'],
          sub: 'Здесь ничего не подключается',
          foot: 'Управлять сетью можно в установленном приложении Zapret Desktop.',
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

  const services = [
    { id: 'youtube', name: 'YouTube', hint: 'Открывается главная страница', icon: 'play' },
    { id: 'discord', name: 'Discord', hint: 'Отвечает API', icon: 'chat' },
    { id: 'udp', name: 'UDP', hint: 'Отвечает STUN-сервер', icon: 'wave' },
    { id: 'cloudflare', name: 'Сайты на Cloudflare', hint: 'Проходит HTTPS', icon: 'globe' },
  ];
  /** Services the background check found broken, by name. */
  const broken = $derived(
    (app.health?.broken ?? []).map((id) => services.find((s) => s.id === id)?.name ?? id),
  );
  type Status = 'ok' | 'warn' | 'bad' | 'wait' | 'idle';
  const labels: Record<Status, string> = {
    ok: 'Доступен',
    warn: 'Мешает DNS',
    bad: 'Не подтверждён',
    wait: 'Проверяем…',
    idle: 'Не проверен',
  };
  const rows = $derived(
    services.map((s) => {
      const result: ProbeResult | undefined = results.find((r) => r.id === s.id);
      const status: Status = result
        ? result.ok
          ? result.warning
            ? 'warn'
            : 'ok'
          : 'bad'
        : op?.busy
          ? 'wait'
          : 'idle';
      return { ...s, result, status };
    }),
  );

  const internet = $derived(results.find((r) => r.id === 'internet'));
  const checked = $derived(rows.filter((r) => r.result));
  const failed = $derived(checked.filter((r) => !r.result?.ok).length);
  const at = $derived(app.checkedAt ? ` в ${formatClock(app.checkedAt)}` : '');
  const summary = $derived.by(() => {
    if (op?.busy) return op.total ? `${op.stage}, ${op.current} из ${op.total}` : op.stage || 'Проверяем…';
    if (internet && !internet.ok) return 'Нет доступа к интернету: остальные результаты не показательны';
    if (!checked.length) return 'Проверка покажет, какие соединения проходят в вашей сети';
    const n = checked.length;
    if (!failed) return `${n === 1 ? 'Проверка пройдена' : `Все ${n} ${plural(n, 'проверка', 'проверки', 'проверок')} пройдены`}${at}`;
    return `${failed} из ${n} ${plural(n, 'проверки', 'проверок', 'проверок')} не прошли${at}`;
  });
</script>

<div class="ov" class:on={mode === 'on'}>
  <section class="slab" aria-live="polite">
    {#if mode === 'on' || mode === 'off'}
      <button
        class="big-toggle"
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
      <span class="slab-icon"><Icon name={mode === 'missing' ? 'download' : 'power'} size={22} /></span>
    {/if}

    <h1 class="slab-title">{slab.title[0]}<br />{slab.title[1]}</h1>
    <p class="slab-sub">{pendingText || slab.sub}</p>

    {#if mode === 'missing'}
      <div class="slab-actions">
        <button class="btn primary" disabled={app.busy} onclick={app.installService}>
          <Icon name="download" size={15} />{app.pending === 'install' ? 'Открываем установщик…' : 'Установить службу'}
        </button>
        <button class="btn ghost" disabled={app.busy} onclick={app.refresh}>Проверить снова</button>
      </div>
    {:else if mode === 'off' && !service?.strategy}
      <div class="slab-actions">
        <button class="btn primary" disabled={app.busy} onclick={app.selectStrategy}>
          <Icon name="wand" size={15} />Подобрать стратегию
        </button>
      </div>
    {/if}

    {#if slab.foot}<p class="slab-foot">{slab.foot}</p>{/if}
  </section>

  <div class="ov-right">
    {#if app.running && broken.length}
      <div class="alert warn health" role="status">
        <Icon name="info" size={16} />
        <p>
          {broken.join(', ')}
          {broken.length > 1 ? 'перестали' : 'перестал'} работать: фоновая проверка в {formatClock(
            new Date((app.health?.checked_at ?? 0) * 1000),
          )} не прошла дважды. Остальное работает.
        </p>
        <button class="btn sm primary" disabled={app.busy} onclick={app.repairStrategy}>
          <Icon name="wand" size={15} />Починить
        </button>
      </div>
    {/if}
    <section class="panel services" aria-labelledby="svc-title">
      {#if op?.busy}
        <div class="progress" class:indeterminate={!op.total}>
          <span style:width={op.total ? `${(op.current / op.total) * 100}%` : null}></span>
        </div>
      {/if}
      <div class="panel-h">
        <div>
          <h2 class="h2" id="svc-title">Доступ к сервисам</h2>
          <p class="sub" class:warn-text={internet && !internet.ok} aria-live="polite">{summary}</p>
        </div>
        <button class="btn sm" disabled={!app.installed || app.busy} onclick={app.diagnose}>
          <Icon name="refresh" size={15} />{op?.busy ? 'Проверяем…' : 'Проверить'}
        </button>
      </div>
      <ul class="svc-list">
        {#each rows as row (row.id)}
          <li class="svc-row" title={row.result?.warning ?? row.result?.detail}>
            <span class="svc-ico"><Icon name={row.icon} size={18} /></span>
            <span class="svc-text">
              <span class="svc-name">{row.name}</span>
              <span class="svc-detail">{row.hint}</span>
            </span>
            <span class="state {row.status}"><i></i>{labels[row.status]}</span>
            <span class="lat num">
              {#if row.result?.ok}{row.result.latency_ms}<small>мс</small>{:else}<span class="dash">—</span>{/if}
            </span>
          </li>
        {/each}
      </ul>
    </section>

    <dl class="panel facts">
      <div class="fact strategy">
        <dt>Стратегия</dt>
        <dd>
          <span class="fact-v mono" title={activeStrategy?.description || service?.strategy}
            >{activeStrategy?.name || service?.strategy || (app.installed ? 'Не выбрана' : '—')}</span
          >
          {#if app.installed && app.bundle}
            <span class="fact-sub" title={app.bundle.error ?? (app.bundle.flowseal ? `Flowseal ${app.bundle.flowseal}` : undefined)}>
              {bundleLine(app.bundle)}
            </span>
          {/if}
          {#if app.installed && service?.strategy}
            <button class="link" disabled={app.busy} onclick={app.selectStrategy}>Подобрать заново</button>
          {/if}
        </dd>
      </div>
      <div class="fact">
        <dt>Движок</dt>
        <dd><span class="fact-v">{service?.engine || '—'}</span></dd>
      </div>
      <div class="fact">
        <dt>DNS</dt>
        <dd>
          <!-- winws reports no packet counters, so this tile shows DNS, which often decides whether a site opens. -->
          {#if service?.secure_dns.enabled}
            <span class="fact-v">{service.secure_dns.provider ?? 'Защищённый'}</span>
            <span class="fact-sub">через HTTPS для всех программ</span>
          {:else}
            <span class="fact-v">Системный</span>
            <span class="fact-sub">{service?.secure_dns.supported ? 'защищённый можно включить в «Настройках»' : 'от провайдера или роутера'}</span>
          {/if}
        </dd>
      </div>
    </dl>
  </div>
</div>

{#if service?.warnings.length}
  <ul class="warnings">
    {#each service.warnings as warning}
      <li class="alert warn"><Icon name="alert" size={16} /><p>{warning}</p></li>
    {/each}
  </ul>
{/if}

<style>
  .ov {
    /* Fill a tall window, but never shrink below the content: the page scrolls instead. */
    flex: 1 0 auto;
    min-height: 470px;
    display: grid;
    grid-template-columns: minmax(300px, 372px) minmax(0, 1fr);
    gap: 16px;
    padding-top: 6px;
  }

  /* The big state card */
  .slab {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: 26px 28px 24px;
    border-radius: 20px;
    background: var(--slab-off);
    border: 1px solid var(--line);
    color: var(--text);
    transition: background-color 0.25s, color 0.25s, border-color 0.25s;
  }
  .on .slab {
    background: var(--g);
    border-color: transparent;
    color: var(--on-g);
  }
  .big-toggle {
    position: relative;
    flex: none;
    width: 104px;
    height: 58px;
    padding: 0;
    border: 0;
    border-radius: 29px;
    background: var(--line-2);
    transition: background-color 0.25s;
  }
  .big-toggle .knob {
    position: absolute;
    top: 7px;
    left: 7px;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    background: var(--text-3);
    transition: transform 0.25s cubic-bezier(0.3, 0.7, 0.4, 1), background-color 0.25s;
  }
  .big-toggle:hover:not(:disabled) .knob {
    background: var(--text-2);
  }
  .big-toggle.on {
    background: var(--on-g);
  }
  .big-toggle.on .knob,
  .big-toggle.on:hover:not(:disabled) .knob {
    transform: translateX(46px);
    background: var(--g);
  }
  .big-toggle.moving .knob {
    transform: translateX(23px);
  }
  .big-toggle:disabled:not(.moving) {
    opacity: 0.6;
  }
  .big-toggle:focus-visible {
    outline-color: currentColor;
    outline-offset: 4px;
    border-radius: 29px;
  }
  .slab-icon {
    width: 58px;
    height: 58px;
    border-radius: 18px;
    display: grid;
    place-items: center;
    background: var(--raise-2);
    color: var(--text-2);
  }
  .slab-title {
    margin-top: auto;
    padding-top: 32px;
    font-size: 56px;
    line-height: 0.96;
    font-weight: 600;
    letter-spacing: -0.04em;
  }
  .slab-sub {
    margin-top: 16px;
    font-size: 15px;
    font-weight: 500;
    opacity: 0.72;
  }
  .slab-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 20px;
  }
  .slab-foot {
    margin-top: 28px;
    padding-top: 16px;
    border-top: 1px solid var(--line-2);
    font-size: 12.5px;
    line-height: 1.5;
    opacity: 0.66;
    max-width: 300px;
  }
  .on .slab-foot {
    border-top-color: rgb(3 20 10 / 0.16);
  }

  /* Services and facts */
  .ov-right {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }
  .services {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .services .progress {
    margin-bottom: -3px;
  }
  .warn-text {
    color: var(--amber);
  }
  .svc-list {
    flex: 1;
    display: grid;
    grid-auto-rows: minmax(64px, 1fr);
    list-style: none;
  }
  .svc-row {
    display: grid;
    grid-template-columns: 38px minmax(0, 1fr) auto 76px;
    align-items: center;
    gap: 14px;
    padding: 0 20px;
    border-top: 1px solid var(--line);
  }
  .svc-ico {
    width: 38px;
    height: 38px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    background: var(--raise);
    border: 1px solid var(--line);
    color: var(--text-2);
  }
  .svc-text {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .svc-name {
    font-size: 14px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .svc-detail {
    margin-top: 1px;
    font-size: 12.5px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .state.wait {
    color: var(--text-2);
  }

  .facts {
    display: grid;
    grid-template-columns: 1.5fr 1fr 1fr;
  }
  .fact {
    padding: 18px 20px;
    border-left: 1px solid var(--line);
    min-width: 0;
  }
  .fact:first-child {
    border-left: 0;
  }
  .fact dt {
    font-size: 12.5px;
    color: var(--text-2);
  }
  .fact dd {
    margin-top: 6px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    min-width: 0;
  }
  .fact-v {
    max-width: 100%;
    font-size: 15px;
    font-weight: 500;
    letter-spacing: -0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .fact-v.mono {
    font-size: 13.5px;
    font-weight: 400;
    letter-spacing: -0.02em;
  }
  .fact-sub {
    font-size: 12px;
    color: var(--text-3);
  }
  .fact .link {
    margin-top: 4px;
  }

  .warnings {
    list-style: none;
    margin-top: 16px;
  }
  .warnings .alert {
    margin: 0 0 8px;
  }

  /* Narrower windows: the card becomes a band above the list. */
  @media (max-width: 940px) {
    .ov {
      grid-template-columns: minmax(0, 1fr);
      min-height: 0;
    }
    .slab {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      grid-template-areas: 'title toggle' 'sub toggle' 'actions actions' 'foot foot';
      align-items: start;
      padding: 22px 24px;
    }
    .slab-title {
      grid-area: title;
      padding-top: 0;
      font-size: 40px;
    }
    .slab-sub {
      grid-area: sub;
      margin-top: 10px;
    }
    .big-toggle,
    .slab-icon {
      grid-area: toggle;
      align-self: center;
    }
    .slab-actions {
      grid-area: actions;
    }
    .slab-foot {
      grid-area: foot;
      max-width: none;
      margin-top: 20px;
    }
    .svc-list {
      grid-auto-rows: 64px;
    }
  }
  @media (max-width: 640px) {
    .slab-title {
      font-size: 32px;
    }
    .facts {
      grid-template-columns: 1fr 1fr;
    }
    .fact.strategy {
      grid-column: 1 / -1;
      border-bottom: 1px solid var(--line);
    }
    .fact:nth-child(2) {
      border-left: 0;
    }
    .svc-row {
      grid-template-columns: 38px minmax(0, 1fr) auto;
    }
    .svc-row .lat {
      display: none;
    }
  }
</style>
