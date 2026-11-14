<script lang="ts">
  import { native } from '../api';
  import Icon from '../Icon.svelte';
  import { app, formatClock, plural } from '../store.svelte';

  const op = $derived(app.operation);
  const order = ['internet', 'youtube', 'discord', 'cloudflare', 'udp'];
  const rank = (id: string) => {
    const i = order.indexOf(id);
    return i < 0 ? order.length : i;
  };
  const results = $derived([...(op?.results ?? [])].sort((a, b) => rank(a.id) - rank(b.id)));
  const failed = $derived(results.filter((r) => !r.ok).length);
  const dnsWarning = $derived(results.some((r) => r.ok && r.warning));

  const summary = $derived.by(() => {
    if (op?.busy) return op.total ? `${op.stage}, ${op.current} из ${op.total}` : op.stage || 'Проверяем…';
    if (!results.length) return 'Узнайте, какие соединения проходят, или подберите стратегию под свою сеть';
    const at = app.checkedAt ? `Завершена в ${formatClock(app.checkedAt)}, ` : '';
    const n = results.length;
    const tail = failed
      ? `${failed} из ${n} ${plural(n, 'соединения', 'соединений', 'соединений')} не проходят`
      : `${n === 1 ? 'соединение проходит' : `все ${n} ${plural(n, 'соединение', 'соединения', 'соединений')} проходят`}`;
    return at ? at + tail : tail.charAt(0).toUpperCase() + tail.slice(1);
  });

  const caps = $derived(app.service?.capabilities);
  const capRows = $derived(
    caps
      ? [
          { label: 'TCP', on: caps.tcp },
          { label: 'UDP', on: caps.udp },
          { label: 'IPv6', on: caps.ipv6 },
          { label: 'Внедрение пакетов', on: caps.packet_injection },
        ]
      : [],
  );
</script>

<header class="page-h">
  <div>
    <h1 class="h1">Проверка</h1>
    <p class="sub" aria-live="polite">{summary}</p>
  </div>
  <div class="actions">
    <button class="btn ghost" disabled={!native} onclick={app.exportReport}>
      <Icon name="download" size={15} />Сохранить отчёт
    </button>
    {#if op?.busy}
      <button class="btn" disabled={app.pending === 'cancel'} onclick={app.cancel}>
        <Icon name="close" size={15} />Отменить
      </button>
    {:else}
      <button class="btn" disabled={!app.installed || app.busy} onclick={app.selectStrategy}>
        <Icon name="wand" size={15} />Подобрать стратегию
      </button>
    {/if}
    <button class="btn primary" disabled={!app.installed || app.busy} onclick={app.diagnose}>
      <Icon name="refresh" size={15} />{results.length ? 'Проверить снова' : 'Проверить'}
    </button>
  </div>
</header>

<div class="chk">
  <div class="main-col">
    {#if op && (op.busy || op.error || op.notice || op.winner)}
      <section class="panel operation" class:failed={!!op.error} aria-live="polite">
        {#if op.busy}
          <div class="progress" class:indeterminate={!op.total}>
            <span style:width={op.total ? `${(op.current / op.total) * 100}%` : null}></span>
          </div>
        {/if}
        <div class="op-head">
          <span class="op-icon" class:spin={op.busy}>
            <Icon name={op.busy ? 'refresh' : op.error ? 'alert' : 'check'} size={16} />
          </span>
          <strong>{op.stage || (op.error ? 'Не получилось' : 'Готово')}</strong>
          {#if op.total}<span class="op-count num">{op.current} из {op.total}</span>{/if}
        </div>
        {#if op.error}<p class="op-text bad">{op.error}</p>{/if}
        {#if op.notice}<p class="op-text">{op.notice}</p>{/if}
        {#if op.winner}
          <p class="op-text good">
            Выбрана и включена стратегия {op.winner}: она дважды прошла все проверки. Видео и голос в Discord проверьте в
            самих приложениях.
          </p>
        {/if}
      </section>
    {/if}

    <section class="panel" aria-label="Результаты проверки">
      {#if results.length}
        <table class="res">
          <thead>
            <tr>
              <th><span class="sr-only">Итог</span></th>
              <th>Соединение</th>
              <th>Протокол</th>
              <th class="right">Задержка</th>
            </tr>
          </thead>
          <tbody>
            {#each results as r (r.id)}
              <tr>
                <td>
                  <span class="mark" class:bad={!r.ok} class:warn={r.ok && r.warning}>
                    <Icon name={!r.ok ? 'close' : r.warning ? 'alert' : 'check'} size={13} stroke={r.warning ? 2.4 : 3} />
                  </span>
                  <span class="sr-only">{!r.ok ? 'Не подтверждена' : r.warning ? 'Прошла с предупреждением' : 'Прошла'}</span>
                </td>
                <td class="what">
                  <span class="name">{r.name}</span>
                  <span class="detail">{r.detail}</span>
                  {#if r.warning}
                    <span class="detail warn-text">{r.warning}</span>
                  {/if}
                </td>
                <td><span class="proto">{r.protocol}</span></td>
                <td class="lat num">
                  {#if r.ok}{r.latency_ms}<small>мс</small>{:else}<span class="dash">—</span>{/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p class="empty">{op?.busy ? 'Результаты появятся по ходу проверки.' : 'Здесь появятся результаты. Нажмите «Проверить».'}</p>
      {/if}
      {#if dnsWarning}
        <div class="alert warn dns-hint">
          <Icon name="info" size={16} />
          <p>
            Чтобы сайты открывались в браузере, включите защищённый DNS. Windows 11: «Параметры» → «Сеть и
            Интернет» → ваше подключение → «Назначение DNS-сервера» → «Изменить» → «Вручную», DNS
            <span class="mono">1.1.1.1</span> и <span class="mono">1.0.0.1</span>, «DNS через HTTPS». В Chrome и
            Edge: «Конфиденциальность и безопасность» → «Использовать безопасный DNS».
          </p>
        </div>
      {/if}
      <p class="note">
        <Icon name="info" size={15} />
        Проверка подтверждает, что соединение проходит. Воспроизведение видео и голос в Discord она не проверяет.
      </p>
    </section>
  </div>

  <div class="side">
    <section class="panel" aria-labelledby="sys-title">
      <div class="panel-h">
        <div>
          <h2 class="h2" id="sys-title">Система</h2>
          <p class="sub">Служба, драйвер, конфликты</p>
        </div>
        <button class="btn sm" disabled={!app.installed || app.busy} onclick={app.checkDoctor}>
          <span class="btn-ico" class:spin={app.pending === 'doctor'}><Icon name="refresh" size={15} /></span
          >{app.pending === 'doctor' ? 'Проверяем…' : 'Проверить'}
        </button>
      </div>
      {#if app.doctorChecks.length}
        <dl class="kv-list">
          {#each app.doctorChecks as check}
            <div class="kv">
              <dt>{check.name}</dt>
              <dd class="state" class:bad={!check.ok}><i></i>{check.detail || (check.ok ? 'В порядке' : 'Есть проблема')}</dd>
            </div>
          {/each}
        </dl>
      {:else}
        <p class="side-empty">
          {app.doctorRan ? 'Служба не вернула ни одной проверки.' : 'Проверит службу, драйвер и конфликтующие программы.'}
        </p>
      {/if}
    </section>

    {#if capRows.length && app.installed}
      <section class="panel" aria-labelledby="caps-title">
        <div class="panel-h">
          <div>
            <h2 class="h2" id="caps-title">Что умеет движок</h2>
            <p class="sub">Зависит от системы и версии движка</p>
          </div>
        </div>
        <ul class="caps">
          {#each capRows as c}
            <li class:off={!c.on}>
              <Icon name={c.on ? 'check' : 'close'} size={14} stroke={2.8} />{c.label}
              <span class="sr-only">{c.on ? 'есть' : 'нет'}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </div>
</div>

<style>
  .chk {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 340px;
    gap: 16px;
    align-items: start;
  }
  .main-col,
  .side {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }

  /* Running or finished operation */
  .operation {
    overflow: hidden;
  }
  .op-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px 20px;
  }
  .op-head strong {
    font-weight: 600;
  }
  .op-icon {
    display: flex;
    color: var(--g-text);
  }
  .operation.failed .op-icon {
    color: var(--red);
  }
  .op-count {
    margin-left: auto;
    color: var(--text-2);
  }
  .op-text {
    padding: 0 20px 16px;
    margin-top: -6px;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-2);
    max-width: 72ch;
  }
  .op-text.good {
    color: var(--g-text);
  }
  .op-text.bad {
    color: var(--red);
  }

  /* Results */
  .res {
    width: 100%;
    border-collapse: collapse;
  }
  .res th {
    height: 38px;
    padding: 0 0 0 14px;
    font-size: 12px;
    font-weight: 400;
    color: var(--text-3);
    text-align: left;
  }
  .res td {
    height: 70px;
    padding: 10px 0 10px 14px;
    border-top: 1px solid var(--line);
    vertical-align: middle;
  }
  .res th:first-child,
  .res td:first-child {
    width: 46px;
    padding-left: 20px;
  }
  .res th:last-child,
  .res td:last-child {
    padding-right: 20px;
  }
  .res .right {
    text-align: right;
  }
  .mark {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--g-soft);
    color: var(--g-text);
  }
  .mark.bad {
    background: var(--red-soft);
    color: var(--red);
  }
  .dns-hint {
    margin: 4px 20px 14px;
    font-size: 12.5px;
  }
  .mark.warn {
    background: var(--amber-soft);
    color: var(--amber);
  }
  .detail.warn-text {
    color: var(--amber);
  }
  .what {
    min-width: 0;
  }
  .name {
    display: block;
    font-size: 14px;
    font-weight: 500;
  }
  .detail {
    display: block;
    margin-top: 1px;
    font-size: 12.5px;
    color: var(--text-2);
    overflow-wrap: anywhere;
  }
  .proto {
    font-family: var(--mono);
    font-size: 11.5px;
    color: var(--text-2);
    border: 1px solid var(--line-2);
    border-radius: 6px;
    padding: 3px 7px;
    white-space: nowrap;
  }
  .res td.lat {
    width: 90px;
  }
  .empty {
    padding: 22px 20px;
    color: var(--text-2);
  }
  .note {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 14px 20px 16px;
    border-top: 1px solid var(--line);
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text-2);
  }
  .note :global(svg) {
    margin-top: 2px;
    color: var(--text-3);
  }

  /* Side panels */
  .btn-ico {
    display: flex;
  }
  .kv-list {
    margin: 0;
  }
  .kv {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 48px;
    padding: 10px 20px;
    border-top: 1px solid var(--line);
    font-size: 13.5px;
  }
  .kv dt {
    color: var(--text-2);
  }
  .kv dd {
    white-space: normal;
    text-align: right;
  }
  .side-empty {
    padding: 14px 20px 18px;
    border-top: 1px solid var(--line);
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text-2);
  }
  .caps {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 2px 20px 20px;
    list-style: none;
  }
  .caps li {
    height: 32px;
    padding: 0 12px 0 10px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    gap: 7px;
    background: var(--raise);
    border: 1px solid var(--line-2);
    font-size: 13px;
    font-weight: 500;
  }
  .caps li :global(svg) {
    color: var(--g-text);
  }
  .caps li.off {
    color: var(--text-3);
  }
  .caps li.off :global(svg) {
    color: var(--text-3);
  }

  @media (max-width: 1000px) {
    .chk {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  @media (max-width: 640px) {
    .res th:nth-child(3),
    .res td:nth-child(3) {
      display: none;
    }
  }
</style>
