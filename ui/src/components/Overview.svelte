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
  /** Monochrome service marks from Simple Icons (CC0); UDP has no brand. */
  const marks: Record<string, string> = {
    youtube: 'M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z',
    discord: 'M20.317 4.3698a19.7913 19.7913 0 00-4.8851-1.5152.0741.0741 0 00-.0785.0371c-.211.3753-.4447.8648-.6083 1.2495-1.8447-.2762-3.68-.2762-5.4868 0-.1636-.3933-.4058-.8742-.6177-1.2495a.077.077 0 00-.0785-.037 19.7363 19.7363 0 00-4.8852 1.515.0699.0699 0 00-.0321.0277C.5334 9.0458-.319 13.5799.0992 18.0578a.0824.0824 0 00.0312.0561c2.0528 1.5076 4.0413 2.4228 5.9929 3.0294a.0777.0777 0 00.0842-.0276c.4616-.6304.8731-1.2952 1.226-1.9942a.076.076 0 00-.0416-.1057c-.6528-.2476-1.2743-.5495-1.8722-.8923a.077.077 0 01-.0076-.1277c.1258-.0943.2517-.1923.3718-.2914a.0743.0743 0 01.0776-.0105c3.9278 1.7933 8.18 1.7933 12.0614 0a.0739.0739 0 01.0785.0095c.1202.099.246.1981.3728.2924a.077.077 0 01-.0066.1276 12.2986 12.2986 0 01-1.873.8914.0766.0766 0 00-.0407.1067c.3604.698.7719 1.3628 1.225 1.9932a.076.076 0 00.0842.0286c1.961-.6067 3.9495-1.5219 6.0023-3.0294a.077.077 0 00.0313-.0552c.5004-5.177-.8382-9.6739-3.5485-13.6604a.061.061 0 00-.0312-.0286zM8.02 15.3312c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9555-2.4189 2.157-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.9555 2.4189-2.1569 2.4189zm7.9748 0c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9554-2.4189 2.1569-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.946 2.4189-2.1568 2.4189Z',
    cloudflare: 'M16.5088 16.8447c.1475-.5068.0908-.9707-.1553-1.3154-.2246-.3164-.6045-.499-1.0615-.5205l-8.6592-.1123a.1559.1559 0 0 1-.1333-.0713c-.0283-.042-.0351-.0986-.021-.1553.0278-.084.1123-.1484.2036-.1562l8.7359-.1123c1.0351-.0489 2.1601-.8868 2.5537-1.9136l.499-1.3013c.0215-.0561.0293-.1128.0147-.168-.5625-2.5463-2.835-4.4453-5.5499-4.4453-2.5039 0-4.6284 1.6177-5.3876 3.8614-.4927-.3658-1.1187-.5625-1.794-.499-1.2026.119-2.1665 1.083-2.2861 2.2856-.0283.31-.0069.6128.0635.894C1.5683 13.171 0 14.7754 0 16.752c0 .1748.0142.3515.0352.5273.0141.083.0844.1475.1689.1475h15.9814c.0909 0 .1758-.0645.2032-.1553l.12-.4268zm2.7568-5.5634c-.0771 0-.1611 0-.2383.0112-.0566 0-.1054.0415-.127.0976l-.3378 1.1744c-.1475.5068-.0918.9707.1543 1.3164.2256.3164.6055.498 1.0625.5195l1.8437.1133c.0557 0 .1055.0263.1329.0703.0283.043.0351.1074.0214.1562-.0283.084-.1132.1485-.204.1553l-1.921.1123c-1.041.0488-2.1582.8867-2.5527 1.914l-.1406.3585c-.0283.0713.0215.1416.0986.1416h6.5977c.0771 0 .1474-.0489.169-.126.1122-.4082.1757-.837.1757-1.2803 0-2.6025-2.125-4.727-4.7344-4.727',
  };
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
    <h1 class="h1">Обзор</h1>
  </header>

  <!-- Properties of this computer, read like the fields of a document. -->
  <dl class="props">
    <div class="prop">
      <dt><Icon name="layers" size={16} />Стратегия</dt>
      <dd>
        <span class="prop-v" title={activeStrategy?.description || service?.strategy}
          >{activeStrategy?.name || service?.strategy || (app.installed ? 'Не выбрана' : '—')}</span
        >
        {#if app.installed && app.bundle}
          <span class="muted" title={app.bundle.error ?? (app.bundle.flowseal ? `Flowseal ${app.bundle.flowseal}` : undefined)}
            >{bundleLine(app.bundle)}</span
          >
        {/if}
        {#if app.installed && service?.strategy}
          <button class="link" disabled={app.busy} onclick={app.selectStrategy}>Подобрать заново</button>
        {/if}
      </dd>
    </div>
    <div class="prop">
      <dt><Icon name="cpu" size={16} />Движок</dt>
      <dd><span class="prop-v">{service?.engine || '—'}</span></dd>
    </div>
    <div class="prop">
      <dt><Icon name="shield" size={16} />DNS</dt>
      <dd>
        <!-- winws reports no packet counters, so DNS is shown: it often decides whether a site opens. -->
        {#if service?.secure_dns.enabled}
          <span class="prop-v">{service.secure_dns.provider ?? 'Защищённый'}</span>
          <span class="muted">через HTTPS для всех программ</span>
        {:else}
          <span class="prop-v">Системный</span>
          <span class="muted">{service?.secure_dns.supported ? 'защищённый можно включить в «Настройках»' : 'от провайдера или роутера'}</span>
        {/if}
      </dd>
    </div>
  </dl>

  <section aria-labelledby="svc-title">
    <div class="section-h">
      <div>
        <h2 class="h2" id="svc-title">Сервисы</h2>
        <p class="sub" class:warn-text={internet && !internet.ok} aria-live="polite">{summary}</p>
      </div>
      <button class="btn sm" disabled={!app.installed || app.busy} onclick={app.diagnose}>
        <Icon name="refresh" size={16} />{op?.busy ? 'Проверяем…' : 'Проверить'}
      </button>
    </div>

    {#if op?.busy}
      <div class="progress" class:indeterminate={!op.total}>
        <span style:width={op.total ? `${(op.current / op.total) * 100}%` : null}></span>
      </div>
    {/if}

    <ul class="svc-list">
      {#each rows as row (row.id)}
        <li class="svc-row {row.status}" title={row.result?.warning ?? row.result?.detail}>
          <span class="svc-ico">
            {#if marks[row.id]}
              <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d={marks[row.id]} /></svg>
            {:else}
              <Icon name={row.icon} size={18} />
            {/if}
          </span>
          <span class="svc-name">{row.name}</span>
          <span class="svc-detail">{row.hint}</span>
          <span class="state {row.status}"><i></i>{labels[row.status]}</span>
          <span class="lat num">
            {#if row.result?.ok}{row.result.latency_ms}<small>мс</small>{:else}<span class="dash">—</span>{/if}
          </span>
        </li>
      {/each}
    </ul>
  </section>

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

  /* Properties: label column and value, a hover surface per row. */
  .props {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0 -8px 40px;
  }
  .prop {
    display: grid;
    grid-template-columns: 160px minmax(0, 1fr);
    align-items: center;
    gap: 12px;
    min-height: 36px;
    padding: 0 8px;
    border-radius: 6px;
  }
  .prop:hover {
    background: var(--hover);
  }
  .prop dt {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
  }
  .prop dd {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 12px;
    min-width: 0;
  }
  .prop-v {
    font-weight: 500;
  }
  .muted {
    font-size: 13px;
    color: var(--text-3);
  }

  .section-h {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    padding-bottom: 12px;
  }
  .progress {
    margin-bottom: 8px;
  }

  /* Services: single-line rows, like items of a list block. */
  .svc-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0 -8px;
    list-style: none;
  }
  .svc-row {
    display: grid;
    grid-template-columns: 20px auto minmax(0, 1fr) auto 64px;
    align-items: center;
    gap: 12px;
    height: 44px;
    padding: 0 8px;
    border-radius: 6px;
    transition: background-color 0.15s;
  }
  .svc-row:hover {
    background: var(--hover);
  }
  .svc-ico {
    display: grid;
    place-items: center;
    color: var(--text-2);
  }
  .svc-name {
    font-size: 15px;
    font-weight: 500;
    white-space: nowrap;
  }
  .svc-detail {
    font-size: 13px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .warnings {
    list-style: none;
    margin-top: 16px;
  }
  .warnings .alert {
    margin: 0 0 8px;
  }

  @media (max-width: 640px) {
    .prop {
      grid-template-columns: 120px minmax(0, 1fr);
    }
    .svc-row {
      grid-template-columns: 20px minmax(0, 1fr) auto;
    }
    .svc-detail,
    .svc-row .lat {
      display: none;
    }
  }
</style>
