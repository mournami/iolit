<script lang="ts">
  import Icon from '../Icon.svelte';
  import { app, bundleLine, formatClock, plural } from '../store.svelte';
  import type { ProbeResult } from '../types';

  const service = $derived(app.service);
  const activeStrategy = $derived(app.data?.strategies.find((s) => s.id === service?.strategy));
  const op = $derived(app.operation);
  const results = $derived(op?.results ?? []);

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

<div class="ov">
  {#if app.running && broken.length}
    <div class="alert warn" role="status">
      <Icon name="info" size={16} />
      <p>
        {broken.join(', ')}
        {broken.length > 1 ? 'перестали' : 'перестал'} работать: фоновая проверка в {formatClock(
          new Date((app.health?.checked_at ?? 0) * 1000),
        )} не прошла дважды. Остальное работает.
      </p>
      <button class="btn sm primary" disabled={app.busy} onclick={app.repairStrategy}>
        <Icon name="wand" size={16} />Починить
      </button>
    </div>
  {/if}
  {#if app.installed && app.appUpdate?.available}
    <div class="alert good" role="status">
      <Icon name="download" size={16} />
      <p>
        {#if app.appUpdate.installing}
          Устанавливаем версию {app.appUpdate.available}. Окно закроется и откроется снова.
        {:else}
          Вышла новая версия приложения {app.appUpdate.available}. Она уже скачана и проверена.
          {#if app.appUpdate.error}<br />{app.appUpdate.error}{/if}
        {/if}
      </p>
      <button class="btn sm primary" disabled={app.busy || app.appUpdate.installing} onclick={app.updateApp}>
        <Icon name="refresh" size={16} />Обновить
      </button>
    </div>
  {/if}

  <header class="page-h">
    <div>
      <h1 class="h1">Доступ к сервисам</h1>
      <p class="sub" class:warn-text={internet && !internet.ok} aria-live="polite">{summary}</p>
    </div>
    <button class="btn" disabled={!app.installed || app.busy} onclick={app.diagnose}>
      <Icon name="refresh" size={16} />{op?.busy ? 'Проверяем…' : 'Проверить'}
    </button>
  </header>

  {#if op?.busy}
    <div class="progress" class:indeterminate={!op.total}>
      <span style:width={op.total ? `${(op.current / op.total) * 100}%` : null}></span>
    </div>
  {/if}

  <ul class="svc-list">
    {#each rows as row (row.id)}
      <li class="svc-row {row.status}" title={row.result?.warning ?? row.result?.detail}>
        <span class="svc-ico"><Icon name={row.icon} size={20} /></span>
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

  <dl class="facts">
    <div class="fact">
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

  {#if service?.warnings.length}
    <ul class="warnings">
      {#each service.warnings as warning}
        <li class="alert warn"><Icon name="alert" size={16} /><p>{warning}</p></li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .ov {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .warn-text {
    color: var(--amber);
  }
  .progress {
    margin-bottom: 8px;
  }

  /* Services: rows on the page itself, no frame and no rules. */
  .svc-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0 -16px;
    list-style: none;
  }
  .svc-row {
    display: grid;
    grid-template-columns: 40px minmax(0, 1fr) auto 72px;
    align-items: center;
    gap: 16px;
    min-height: 72px;
    padding: 0 16px;
    border-radius: 16px;
    transition: background-color 0.15s;
  }
  .svc-row:hover {
    background: var(--hover);
  }
  .svc-ico {
    width: 40px;
    height: 40px;
    border-radius: 12px;
    display: grid;
    place-items: center;
    background: var(--raise);
    color: var(--text-2);
  }
  .svc-row.ok .svc-ico {
    background: var(--ok-soft);
    color: var(--ok);
  }
  .svc-row.bad .svc-ico {
    background: var(--red-soft);
    color: var(--red);
  }
  .svc-text {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .svc-name {
    font-size: 15px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .svc-detail {
    margin-top: 2px;
    font-size: 13px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .state.wait {
    color: var(--text-2);
  }
  .lat {
    font-family: var(--mono);
    font-size: 14px;
  }

  /* Facts: three figures on one soft band. */
  .facts {
    display: grid;
    grid-template-columns: 1.5fr 1fr 1fr;
    gap: 4px;
    margin-top: 32px;
    padding: 20px 24px;
    border-radius: 20px;
    background: var(--panel);
  }
  .fact {
    min-width: 0;
  }
  .fact dt {
    font-size: 13px;
    color: var(--text-2);
  }
  .fact dd {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    min-width: 0;
  }
  .fact-v {
    max-width: 100%;
    font-size: 16px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .fact-v.mono {
    font-size: 14px;
    font-weight: 400;
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

  @media (max-width: 640px) {
    .facts {
      grid-template-columns: 1fr 1fr;
      gap: 16px;
    }
    .fact:first-child {
      grid-column: 1 / -1;
    }
    .svc-row {
      grid-template-columns: 40px minmax(0, 1fr) auto;
    }
    .svc-row .lat {
      display: none;
    }
  }
</style>
