<script lang="ts">
  // What used to be the «Проверка» page beyond the service list: the system
  // check (Doctor), what the engine can do and the full report.
  import { native } from '../api';
  import Icon from '../Icon.svelte';
  import { app } from '../store.svelte';

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

<div class="details">
  <section aria-labelledby="sys-title">
    <div class="section-h">
      <div>
        <h3 class="h3" id="sys-title">Система</h3>
        <p class="sub">Служба, движок, драйвер WinDivert, другие программы обхода и DNS</p>
      </div>
      <button class="btn sm" disabled={!app.installed || app.busy} onclick={app.checkDoctor}>
        <span class="btn-ico" class:spin={app.pending === 'doctor'}><Icon name="refresh" size={16} /></span
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
    {:else if app.doctorRan}
      <p class="empty">Служба не вернула ни одной проверки.</p>
    {/if}
  </section>

  {#if capRows.length && app.installed}
    <section aria-labelledby="caps-title">
      <div class="section-h">
        <div>
          <h3 class="h3" id="caps-title">Что умеет движок</h3>
          <p class="sub">Зависит от системы и версии движка</p>
        </div>
      </div>
      <ul class="caps">
        {#each capRows as c}
          <li class:off={!c.on}>
            <Icon name={c.on ? 'check' : 'close'} size={14} stroke={2} />{c.label}
            <span class="sr-only">{c.on ? 'есть' : 'нет'}</span>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  <section aria-labelledby="report-title">
    <div class="section-h">
      <div>
        <h3 class="h3" id="report-title">Отчёт</h3>
        <p class="sub">Всё для поиска поломки в одном файле. IP-адреса и ваши домены в нём скрыты.</p>
      </div>
      <button class="btn sm" disabled={!native} onclick={app.exportReport}>
        <Icon name="download" size={16} />Сохранить отчёт
      </button>
    </div>
  </section>

  <p class="note">
    <Icon name="info" size={16} />
    Проверка подтверждает, что соединение проходит. Воспроизведение видео и голос в Discord она не проверяет.
  </p>
</div>

<style>
  .details {
    display: flex;
    flex-direction: column;
    gap: 24px;
    padding-top: 8px;
  }
  .section-h {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .h3 {
    font-size: 15px;
    line-height: 20px;
    font-weight: 600;
  }
  .btn-ico {
    display: flex;
  }
  .kv-list {
    margin: 8px -8px 0;
  }
  .kv {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 36px;
    padding: 4px 8px;
    border-radius: 6px;
  }
  .kv:hover {
    background: var(--hover);
  }
  .kv dt {
    color: var(--text-2);
  }
  .empty {
    margin-top: 8px;
    color: var(--text-2);
  }
  .caps {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 12px;
    list-style: none;
  }
  .caps li {
    height: 28px;
    padding: 0 10px 0 8px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--raise-2);
    font-size: 13px;
    font-weight: 500;
  }
  .caps li :global(svg) {
    color: var(--ok);
  }
  .caps li.off,
  .caps li.off :global(svg) {
    color: var(--text-3);
  }
  .note {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    font-size: 13px;
    color: var(--text-2);
  }
  .note :global(svg) {
    margin-top: 2px;
    color: var(--text-3);
  }
</style>
