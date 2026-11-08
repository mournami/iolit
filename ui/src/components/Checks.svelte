<script lang="ts">
  import { native } from '../api';
  import Icon from '../Icon.svelte';
  import { app } from '../store.svelte';

  const op = $derived(app.operation);
  const order = ['internet', 'youtube', 'discord', 'cloudflare', 'udp'];
  const rank = (id: string) => {
    const i = order.indexOf(id);
    return i < 0 ? order.length : i;
  };
  const results = $derived([...(op?.results ?? [])].sort((a, b) => rank(a.id) - rank(b.id)));
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

<header class="page-head">
  <div>
    <h1>Проверка</h1>
    <p>Узнайте, какие соединения проходят, или подберите стратегию под свою сеть.</p>
  </div>
  <button class="btn btn-secondary" disabled={!native} onclick={app.exportReport}>
    <Icon name="download" size={16} />Сохранить отчёт
  </button>
</header>

<div class="toolbar">
  <button class="btn btn-primary" disabled={!native || app.busy} onclick={app.diagnose}>
    <Icon name="pulse" size={16} />Проверить соединения
  </button>
  <button class="btn btn-secondary" disabled={!app.installed || app.busy} onclick={app.selectStrategy}>
    <Icon name="sliders" size={16} />Подобрать стратегию
  </button>
  {#if op?.busy}
    <button class="btn btn-ghost" disabled={app.pending === 'cancel'} onclick={app.cancel}>Отменить</button>
  {/if}
</div>

{#if op && (op.busy || op.stage)}
  <section class="panel operation" class:failed={!!op.error} aria-live="polite">
    <div class="op-head">
      <span class="op-icon" class:spin={op.busy}>
        <Icon name={op.busy ? 'refresh' : op.error ? 'alert' : 'check'} size={18} />
      </span>
      <strong>{op.stage}</strong>
      {#if op.total}<span class="op-count">{op.current} из {op.total}</span>{/if}
    </div>
    {#if op.busy}
      <div class="progress" class:indeterminate={!op.total}>
        <span style:width={op.total ? `${(op.current / op.total) * 100}%` : null}></span>
      </div>
    {/if}
    {#if op.error}<p class="text-fault">{op.error}</p>{/if}
    {#if op.notice}<p class="text-signal">{op.notice}</p>{/if}
    {#if op.winner}
      <p class="text-signal">
        Выбрана и включена стратегия {op.winner}: она дважды прошла все проверки. Видео и голос в Discord проверьте в самих
        приложениях.
      </p>
    {/if}
  </section>
{/if}

<section class="panel" aria-labelledby="results-title">
  <div class="panel-head">
    <div>
      <h2 id="results-title">Результаты</h2>
      <p>Проверяются HTTPS-соединения и UDP через STUN.</p>
    </div>
  </div>
  {#if results.length}
    <table class="rows">
      <thead class="sr-only">
        <tr><th>Итог</th><th>Проверка</th><th>Протокол</th><th>Время ответа</th></tr>
      </thead>
      <tbody>
        {#each results as r (r.id)}
          <tr class:ok={r.ok}>
            <td class="state">
              <span class="state-icon"><Icon name={r.ok ? 'check' : 'alert'} size={16} /></span>
              <span class="sr-only">{r.ok ? 'Прошла' : 'Не подтверждена'}</span>
            </td>
            <td class="what"><strong>{r.name}</strong><p>{r.detail}</p></td>
            <td class="proto">{r.protocol}</td>
            <td class="num">{r.latency_ms} мс</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <p class="empty">Здесь появятся результаты. Нажмите «Проверить соединения».</p>
  {/if}
</section>

<section class="panel" aria-labelledby="doctor-title">
  <div class="panel-head">
    <div>
      <h2 id="doctor-title">Состояние системы</h2>
      <p>Служба, движок и сетевые правила на этом компьютере.</p>
    </div>
    <button class="btn btn-secondary" disabled={!app.installed || app.busy} onclick={app.checkDoctor}>
      {app.pending === 'doctor' ? 'Проверяем…' : 'Проверить систему'}
    </button>
  </div>
  {#if app.doctorChecks.length}
    <table class="rows">
      <thead class="sr-only"><tr><th>Итог</th><th>Компонент</th><th>Подробности</th></tr></thead>
      <tbody>
        {#each app.doctorChecks as check}
          <tr class:ok={check.ok}>
            <td class="state">
              <span class="state-icon"><Icon name={check.ok ? 'check' : 'alert'} size={16} /></span>
              <span class="sr-only">{check.ok ? 'В порядке' : 'Есть проблема'}</span>
            </td>
            <td class="what"><strong>{check.name}</strong></td>
            <td class="detail">{check.detail || '—'}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else if app.doctorRan}
    <p class="empty">Служба не вернула ни одной проверки.</p>
  {/if}
</section>

{#if capRows.length && app.installed}
  <section class="panel" aria-labelledby="caps-title">
    <div class="panel-head">
      <div>
        <h2 id="caps-title">Что умеет движок</h2>
        <p>Зависит от системы и установленного движка.</p>
      </div>
    </div>
    <dl class="caps">
      {#each capRows as c}
        <div class:on={c.on}>
          <dt>{c.label}</dt>
          <dd><Icon name={c.on ? 'check' : 'close'} size={14} />{c.on ? 'Есть' : 'Нет'}</dd>
        </div>
      {/each}
    </dl>
  </section>
{/if}

<p class="footnote">
  Успешная проверка HTTPS или STUN подтверждает конкретное соединение. Она не подтверждает воспроизведение видео, голос в
  Discord или работу любого другого приложения.
</p>

<style>
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin-bottom: 20px;
  }

  .operation {
    position: relative;
    overflow: hidden;
  }
  .op-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .op-head strong {
    font-weight: 560;
  }
  .op-icon {
    display: flex;
    color: var(--ink-2);
  }
  .operation:not(.failed) .op-icon:not(.spin) {
    color: var(--signal);
  }
  .operation.failed .op-icon {
    color: var(--fault);
  }
  .op-count {
    margin-left: auto;
    color: var(--ink-2);
    font-variant-numeric: tabular-nums;
  }
  .operation .progress {
    margin-top: 14px;
  }
  .operation p {
    margin-top: 12px;
    line-height: 1.5;
    max-width: 72ch;
  }
  .spin {
    animation: spin 1.2s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .rows {
    width: 100%;
    border-collapse: collapse;
    margin-top: 6px;
  }
  .rows td {
    padding: 12px 0;
    border-top: 1px solid var(--line);
    vertical-align: top;
  }
  .rows td + td {
    padding-left: 14px;
  }
  .state {
    width: 28px;
  }
  .state-icon {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--caution);
    background: var(--caution-wash);
  }
  .ok .state-icon {
    color: var(--signal);
    background: var(--signal-wash);
  }
  .what strong {
    font-weight: 560;
    display: block;
    line-height: 26px;
  }
  .what p {
    color: var(--ink-3);
    font-size: 13px;
    line-height: 1.45;
    overflow-wrap: anywhere;
    max-width: 64ch;
  }
  .proto,
  .num,
  .detail {
    line-height: 26px;
    color: var(--ink-2);
    white-space: nowrap;
  }
  .num {
    text-align: right;
    width: 1%;
  }
  .proto {
    width: 1%;
    font-size: 13px;
  }
  .detail {
    white-space: normal;
    text-align: right;
    font-size: 13px;
  }

  .empty {
    color: var(--ink-3);
    padding: 18px 0 4px;
    border-top: 1px solid var(--line);
    margin-top: 6px;
  }

  .caps {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 1px;
    margin: 6px 0 0;
    border-top: 1px solid var(--line);
    padding-top: 14px;
  }
  .caps dt {
    font-size: 13px;
    color: var(--ink-2);
  }
  .caps dd {
    margin: 4px 0 0;
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--ink-3);
    font-weight: 560;
  }
  .caps .on dd {
    color: var(--signal);
  }

  .footnote {
    font-size: 12px;
    color: var(--ink-3);
    line-height: 1.5;
    max-width: 80ch;
    padding: 0 4px;
  }

  @media (max-width: 700px) {
    .proto {
      display: none;
    }
    .caps {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      row-gap: 12px;
    }
  }
</style>
