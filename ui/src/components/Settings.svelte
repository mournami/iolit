<script lang="ts">
  import Icon from '../Icon.svelte';
  import { app } from '../store.svelte';
  import Mark from './Mark.svelte';

  let selected = $state('');
  let domain = $state('');

  // Follow the strategy the service reports; a failed change snaps back on the next refresh.
  $effect(() => {
    selected = app.service?.strategy ?? '';
  });

  async function submitDomain(e: SubmitEvent) {
    e.preventDefault();
    if (await app.addDomain(domain)) domain = '';
  }
</script>

<header class="page-head">
  <div>
    <h1>Настройки</h1>
    <p>Стратегия, ваши домены и журнал службы.</p>
  </div>
</header>

<section class="panel group">
  <div class="row">
    <div class="row-text">
      <h2 id="strategy-label">Стратегия</h2>
      <p>Набор правил, по которым меняются пакеты. Список зависит от установленного движка.</p>
    </div>
    <span class="select">
      <select
        aria-labelledby="strategy-label"
        bind:value={selected}
        disabled={!app.installed || app.busy}
        onchange={() => app.setStrategy(selected)}
      >
        <option value="" disabled>Выберите стратегию</option>
        {#each app.data?.strategies ?? [] as s (s.id)}
          <option value={s.id} disabled={!s.supported}>{s.name}{s.supported ? '' : ' (несовместима)'}</option>
        {/each}
      </select>
    </span>
  </div>
  <div class="row">
    <div class="row-text">
      <h2>После перезапуска компьютера</h2>
      <p>Служба вернёт последнее состояние. Если вы выключили обход, он останется выключенным.</p>
    </div>
    <span class="value">Как было</span>
  </div>
  <div class="row">
    <div class="row-text">
      <h2>Телеметрия</h2>
      <p>Приложение не отправляет статистику, историю и отчёты. Отчёт сохраняется только по вашей команде.</p>
    </div>
    <span class="value">Выключена</span>
  </div>
</section>

<section class="panel" aria-labelledby="domains-title">
  <div class="row-text">
    <h2 id="domains-title">Свои домены</h2>
    <p>Добавьте сайт, который не открывается. Он попадёт в ваш отдельный список, общий список не меняется.</p>
  </div>
  <form class="domain-form" onsubmit={submitDomain}>
    <input
      aria-label="Домен сайта"
      placeholder="example.com"
      bind:value={domain}
      maxlength={253}
      required
      autocomplete="off"
      spellcheck="false"
    />
    <button class="btn btn-secondary" type="submit" disabled={!app.installed || app.busy || app.domainBusy || !domain.trim()}>
      <Icon name="plus" size={16} />Добавить
    </button>
  </form>
</section>

<section class="panel" aria-labelledby="log-title">
  <div class="panel-head">
    <div class="row-text">
      <h2 id="log-title">Журнал службы</h2>
      <p>Последние 80 строк. Журнал хранится только на этом компьютере и может содержать сетевые адреса.</p>
    </div>
    <button class="btn btn-secondary" disabled={!app.installed} aria-expanded={app.showLogs} onclick={app.toggleLogs}>
      <Icon name="terminal" size={16} />{app.showLogs ? 'Скрыть' : 'Показать'}
    </button>
  </div>
  {#if app.showLogs}
    <pre class="log">{app.logLines.join('\n') || 'Журнал пуст.'}</pre>
  {/if}
</section>

<footer class="about">
  <Mark size={28} />
  <div>
    <strong>Zapret Desktop {app.data?.version || '0.1.0'}</strong>
    <p>Открытый исходный код, лицензия MIT. Компоненты движков распространяются по своим лицензиям.</p>
  </div>
</footer>

<style>
  .group {
    padding-top: 6px;
    padding-bottom: 6px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 16px 0;
  }
  .row + .row {
    border-top: 1px solid var(--line);
  }
  .row-text h2 {
    font-size: 15px;
  }
  .row-text p {
    margin-top: 4px;
    font-size: 13px;
    line-height: 1.45;
    color: var(--ink-3);
    max-width: 58ch;
  }
  .value {
    color: var(--ink-2);
    white-space: nowrap;
  }

  .select {
    position: relative;
    flex: none;
  }
  .select::after {
    content: '';
    position: absolute;
    right: 13px;
    top: 50%;
    width: 7px;
    height: 7px;
    border-right: 1.75px solid var(--ink-2);
    border-bottom: 1.75px solid var(--ink-2);
    transform: translateY(-70%) rotate(45deg);
    pointer-events: none;
  }
  select {
    appearance: none;
    width: 240px;
    padding-right: 34px;
    text-overflow: ellipsis;
  }

  .domain-form {
    display: flex;
    gap: 10px;
    margin-top: 16px;
  }
  .domain-form input {
    flex: 1;
    min-width: 0;
    max-width: 420px;
  }

  .log {
    margin: 16px 0 0;
    padding: 14px 16px;
    max-height: 320px;
    overflow: auto;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 8px;
    font-family: ui-monospace, 'SF Mono', Menlo, Consolas, monospace;
    font-size: 12px;
    line-height: 1.6;
    color: var(--ink-2);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .about {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 28px;
    padding: 0 4px;
    color: var(--ink-3);
  }
  .about strong {
    color: var(--ink);
    font-weight: 560;
  }
  .about p {
    font-size: 12px;
    margin-top: 2px;
  }

  @media (max-width: 860px) {
    .row {
      flex-direction: column;
      align-items: flex-start;
      gap: 12px;
    }
  }
</style>
