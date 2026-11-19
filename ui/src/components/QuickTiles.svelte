<script lang="ts">
  // Switches people reach for most, brought up from Settings: the Telegram
  // proxy, secure DNS and their own sites. Settings keeps the full versions.
  import { onMount } from 'svelte';
  import Icon from '../Icon.svelte';
  import { app, plural } from '../store.svelte';

  const TELEGRAM =
    'M11.944 0A12 12 0 0 0 0 12a12 12 0 0 0 12 12 12 12 0 0 0 12-12A12 12 0 0 0 12 0a12 12 0 0 0-.056 0zm4.962 7.224c.1-.002.321.023.465.14a.506.506 0 0 1 .171.325c.016.093.036.306.02.472-.18 1.898-.962 6.502-1.36 8.627-.168.9-.499 1.201-.82 1.23-.696.065-1.225-.46-1.9-.902-1.056-.693-1.653-1.124-2.678-1.8-1.185-.78-.417-1.21.258-1.91.177-.184 3.247-2.977 3.307-3.23.007-.032.014-.15-.056-.212s-.174-.041-.249-.024c-.106.024-1.793 1.14-5.061 3.345-.48.33-.913.49-1.302.48-.428-.008-1.252-.241-1.865-.44-.752-.245-1.349-.374-1.297-.789.027-.216.325-.437.893-.663 3.498-1.524 5.83-2.529 6.998-3.014 3.332-1.386 4.025-1.627 4.476-1.635z';

  const tg = $derived(app.telegram);
  const dns = $derived(app.service?.secure_dns);

  const tgLine = $derived(
    !tg?.enabled
      ? 'Выключен'
      : tg.error
        ? tg.error
        : tg.running
          ? tg.active
            ? `Работает, ${tg.active} ${plural(tg.active, 'подключение', 'подключения', 'подключений')}`
            : 'Работает, Telegram ещё не подключился'
          : 'Запускается…',
  );
  const dnsLine = $derived(
    !dns?.supported
      ? 'Для всей системы есть только в Windows 11'
      : dns.enabled
        ? `${dns.provider ?? 'Публичный DNS'} через HTTPS`
        : 'Адреса сайтов отдаёт провайдер',
  );
  const sitesLine = $derived(
    app.domains.length
      ? `${app.domains.length} ${plural(app.domains.length, 'сайт', 'сайта', 'сайтов')} в вашем списке`
      : 'Пока только встроенные списки',
  );

  onMount(() => {
    void app.loadDomains();
  });
</script>

<section class="quick" aria-labelledby="quick-title">
  <h2 class="h2" id="quick-title">Быстрые настройки</h2>
  <div class="tiles">
    <article class="tile" class:on={tg?.enabled}>
      <span class="tile-ico">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d={TELEGRAM} /></svg>
      </span>
      <h3 class="tile-t">Прокси для Telegram</h3>
      <p class="tile-d" class:bad-text={!!tg?.error}>{tgLine}</p>
      <div class="tile-a">
        <button class="btn sm" disabled={!app.installed || app.busy} onclick={() => app.setTelegram(!tg?.enabled)}>
          {app.pending === 'telegram' ? 'Подождите…' : tg?.enabled ? 'Выключить' : 'Включить'}
        </button>
        {#if tg?.enabled && tg.link}
          <button class="link" onclick={app.openTelegram}>Открыть в Telegram</button>
        {/if}
      </div>
    </article>

    <article class="tile" class:on={dns?.enabled}>
      <span class="tile-ico"><Icon name="shield" size={18} /></span>
      <h3 class="tile-t">Защищённый DNS</h3>
      <p class="tile-d">{dnsLine}</p>
      <div class="tile-a">
        {#if dns?.supported}
          <button class="btn sm" disabled={!app.installed || app.busy} onclick={() => app.setSecureDns(!dns.enabled)}>
            {app.pending === 'dns' ? 'Подождите…' : dns.enabled ? 'Выключить' : 'Включить'}
          </button>
        {/if}
      </div>
    </article>

    <article class="tile">
      <span class="tile-ico"><Icon name="globe" size={18} /></span>
      <h3 class="tile-t">Свои сайты</h3>
      <p class="tile-d">{sitesLine}</p>
      <div class="tile-a">
        <button class="btn sm" onclick={() => (app.page = 'settings')}>
          <Icon name="plus" size={16} />Добавить сайт
        </button>
      </div>
    </article>
  </div>
</section>

<style>
  .quick {
    margin-bottom: 32px;
  }
  .h2 {
    padding-bottom: 12px;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(176px, 1fr));
    gap: 12px;
  }
  /* A soft surface, no frame: the tile only groups a switch with its state. */
  .tile {
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr);
    grid-template-areas: 'ico title' '. desc' '. act';
    column-gap: 12px;
    align-content: start;
    min-height: 136px;
    padding: 16px;
    border-radius: 12px;
    background: var(--panel);
  }
  .tile-ico {
    grid-area: ico;
    display: grid;
    place-items: center;
    height: 20px;
    color: var(--text-2);
  }
  .tile.on .tile-ico {
    color: var(--ok);
  }
  .tile-t {
    grid-area: title;
    font-size: 15px;
    line-height: 20px;
    font-weight: 600;
  }
  .tile-d {
    grid-area: desc;
    margin-top: 4px;
    font-size: 13px;
    line-height: 20px;
    color: var(--text-2);
  }
  .bad-text {
    color: var(--red);
  }
  .tile-a {
    grid-area: act;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin-top: 16px;
  }
</style>
