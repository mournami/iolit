<script lang="ts">
  import Icon from '../Icon.svelte';
  import { app, bundleLine, plural } from '../store.svelte';
  import { theme, type ThemeChoice } from '../theme.svelte';
  import { appearance, DEFAULT_ACCENT, DEFAULT_BG } from '../appearance.svelte';
  import ColorPicker from './ColorPicker.svelte';
  import FontPicker from './FontPicker.svelte';
  import IconCropper from './IconCropper.svelte';
  import Mark from './Mark.svelte';

  let cropping = $state(false);
  const BG_PRESETS = [
    '#000000', '#0b0d0c', '#0d1117', '#111827', '#14161f', '#1a1b26', '#1e1e2e', '#1f1d2b',
    '#18181b', '#202225', '#1b2420', '#2b1d24', '#ffffff', '#f6f7f6', '#f5f1e8', '#eef2f7',
  ];
  const ACCENT_PRESETS = [
    '#3ee387', '#22c55e', '#a3e635', '#facc15', '#f59e0b', '#fb923c', '#f87171', '#f472b6',
    '#e879f9', '#a78bfa', '#818cf8', '#60a5fa', '#38bdf8', '#22d3ee', '#2dd4bf', '#f2f5f3',
  ];

  let selected = $state('');
  const dns = $derived(app.service?.secure_dns);
  const tg = $derived(app.telegram);
  let domain = $state('');

  // Load the lists once the service answers, not on every refresh.
  const installed = $derived(app.installed);
  $effect(() => {
    if (installed) {
      app.loadDomains();
      app.loadLogs();
    }
  });

  // Follow the strategy the service reports; a failed change snaps back on the next refresh.
  $effect(() => {
    selected = app.service?.strategy ?? '';
  });

  const themes: { id: ThemeChoice; label: string; icon: string }[] = [
    { id: 'dark', label: 'Тёмная', icon: 'moon' },
    { id: 'light', label: 'Светлая', icon: 'sun' },
    { id: 'system', label: 'Как в системе', icon: 'monitor' },
  ];

  const updates = $derived.by(() => {
    const b = app.bundle;
    if (!b) return { value: '—', text: 'Станет известно, когда служба ответит' };
    if (!b.updates_enabled) return { value: 'Выключены', text: 'В этой сборке нет ключа проверки, используются стратегии из установщика' };
    return { value: b.error ? 'Ошибка' : 'Включены', text: b.error ?? bundleLine(b) };
  });

  async function submitDomain(e: SubmitEvent) {
    e.preventDefault();
    if (await app.addDomain(domain)) domain = '';
  }
</script>

<header class="page-h">
  <h1 class="h1">Настройки</h1>
</header>

<div class="set">
  <div class="col left">
    <section class="panel rows" aria-label="Работа обхода">
      <div class="row">
        <div>
          <h2 class="row-k" id="strategy-label">Стратегия</h2>
          <p class="row-d">Набор правил, по которым меняются пакеты</p>
        </div>
        <span class="select">
          <select
            class="mono"
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
          <Icon name="chevron" size={15} />
        </span>
      </div>
      <div class="row">
        <div>
          <h2 class="row-k">После перезапуска компьютера</h2>
          <p class="row-d">Служба вернёт последнее состояние. Если вы выключили обход, он останется выключенным.</p>
        </div>
        <span class="value">Как было</span>
      </div>
      <div class="row">
        <div>
          <h2 class="row-k">Защищённый DNS</h2>
          <p class="row-d">
            {#if !dns?.supported}
              Для всей системы есть только в Windows 11. Включите безопасный DNS в браузере.
            {:else if dns.enabled}
              Адреса сайтов приходят через HTTPS от {dns.provider ?? 'публичного DNS'}. Выключение вернёт прежние настройки.
            {:else}
              Помогает, если провайдер подменяет или блокирует адреса сайтов. Cloudflare, Google или Quad9 через HTTPS:
              программа берёт первый, который отвечает в вашей сети. Прежние настройки сохранятся.
            {/if}
          </p>
        </div>
        {#if dns?.supported}
          <button
            class="btn sm"
            class:primary={!dns.enabled}
            disabled={!app.installed || app.busy}
            onclick={() => app.setSecureDns(!dns.enabled)}
          >
            {app.pending === 'dns' ? 'Подождите…' : dns.enabled ? 'Выключить' : 'Включить'}
          </button>
        {:else}
          <span class="value">Недоступен</span>
        {/if}
      </div>
      <div class="row">
        <div>
          <h2 class="row-k">Обновления стратегий</h2>
          <p class="row-d">{updates.text}</p>
        </div>
        <span class="value" class:bad={updates.value === 'Ошибка'}>{updates.value}</span>
      </div>
    </section>

    <section class="panel rows" aria-labelledby="tg-title">
      <div class="row">
        <div>
          <h2 class="row-k" id="tg-title">Telegram</h2>
          <p class="row-d">
            {#if tg?.running}
              Прокси работает на этом компьютере, порт 1443. Перед выключением отключите его в Telegram:
              «Настройки» → «Продвинутые настройки» → «Тип соединения».
            {:else}
              Локальный прокси для Telegram Desktop: трафик идёт по WebSocket напрямую к серверам Telegram, без
              сторонних серверов. Работает и при выключенном обходе.
            {/if}
          </p>
          {#if tg?.error}<p class="row-d bad">{tg.error}</p>{/if}
        </div>
        <button
          class="btn sm"
          class:primary={!tg?.enabled}
          disabled={!app.installed || app.pending !== null}
          onclick={() => app.setTelegram(!tg?.enabled)}
        >
          {app.pending === 'telegram' ? 'Подождите…' : tg?.enabled ? 'Выключить' : 'Включить'}
        </button>
      </div>
      {#if tg?.running}
        <div class="row">
          <div>
            <h2 class="row-k">Подключить Telegram</h2>
            <p class="row-d">
              Нажмите «Открыть в Telegram» и подтвердите прокси. Если окно не появилось, скопируйте ссылку, отправьте
              её себе в «Избранное» и нажмите на неё. Достаточно одного раза.
            </p>
            <p class="row-d">
              {#if tg.active}
                Сейчас {tg.active} {plural(tg.active, 'соединение', 'соединения', 'соединений')} Telegram.
              {:else}
                Telegram пока не подключён к прокси.
              {/if}
              {#if tg.websocket || tg.direct || tg.failed}
                С момента включения: через WebSocket {tg.websocket}, напрямую {tg.direct}, не удалось {tg.failed}.
              {/if}
            </p>
          </div>
          <div class="icon-actions">
            <button class="btn sm ghost" onclick={app.copyTelegramLink}>
              <Icon name="copy" size={15} />Копировать ссылку
            </button>
            <button class="btn sm" onclick={app.openTelegram}>
              <Icon name="send" size={15} />Открыть в Telegram
            </button>
          </div>
        </div>
      {/if}
    </section>

    <section class="panel rows" aria-labelledby="look-title">
      <div class="row head-row">
        <h2 class="h2" id="look-title">Оформление</h2>
        {#if appearance.custom}
          <button class="link" onclick={appearance.reset}>Сбросить всё</button>
        {/if}
      </div>
      <div class="row">
        <div>
          <h2 class="row-k" id="theme-label">Тема</h2>
          {#if appearance.bg}<p class="row-d">Сейчас тему задаёт свой цвет фона</p>{/if}
        </div>
        <div class="seg" role="radiogroup" aria-labelledby="theme-label">
          {#each themes as t (t.id)}
            <button
              role="radio"
              aria-checked={!appearance.bg && theme.choice === t.id}
              class:on={!appearance.bg && theme.choice === t.id}
              onclick={() => theme.set(t.id)}
            >
              <Icon name={t.icon} size={14} />{t.label}
            </button>
          {/each}
        </div>
      </div>
      <div class="row">
        <div>
          <h2 class="row-k">Фон</h2>
          <p class="row-d">Основной цвет окна. Панели, линии и текст подстраиваются под него.</p>
        </div>
        <ColorPicker
          label="Цвет фона"
          value={appearance.bg}
          fallback={theme.resolved === 'light' ? '#ffffff' : DEFAULT_BG}
          presets={BG_PRESETS}
          onchange={appearance.setBg}
        />
      </div>
      <div class="row">
        <div>
          <h2 class="row-k">Акцент</h2>
          <p class="row-d">Цвет включённого обхода, кнопок и отметок «работает».</p>
        </div>
        <ColorPicker
          label="Цвет акцента"
          value={appearance.accent}
          fallback={DEFAULT_ACCENT}
          presets={ACCENT_PRESETS}
          against={appearance.surface(theme.resolved)}
          onchange={appearance.setAccent}
        />
      </div>
      <div class="row">
        <div>
          <h2 class="row-k">Шрифт</h2>
          <p class="row-d">Журнал и коды остаются моноширинными.</p>
        </div>
        <FontPicker value={appearance.font} onchange={appearance.setFont} />
      </div>
      <div class="row">
        <div class="icon-row">
          <span class="icon-preview"><Mark size={36} /></span>
          <div>
            <h2 class="row-k">Значок</h2>
            <p class="row-d">Картинка слева от названия в шапке. Любой размер, обрежете квадрат сами.</p>
            {#if appearance.storageError}<p class="row-d bad">{appearance.storageError}</p>{/if}
          </div>
        </div>
        <div class="icon-actions">
          {#if appearance.icon}
            <button class="btn sm ghost" onclick={() => appearance.setIcon(null)}>Убрать</button>
          {/if}
          <button class="btn sm" onclick={() => (cropping = true)}>
            <Icon name="upload" size={15} />{appearance.icon ? 'Заменить' : 'Загрузить'}
          </button>
        </div>
      </div>
    </section>

    <section class="panel rows" aria-label="Данные">
      <div class="row">
        <div>
          <h2 class="row-k">Телеметрия</h2>
          <p class="row-d">Статистика, история и отчёты не отправляются. Отчёт сохраняется только по вашей команде.</p>
        </div>
        <span class="value locked"><Icon name="lock" size={15} />Выключена</span>
      </div>
    </section>

  </div>

  <div class="col right">
    <section class="panel" aria-labelledby="domains-title">
      <div class="panel-h">
        <div>
          <h2 class="h2" id="domains-title">Свои домены</h2>
          <p class="sub">Сайты, которые у вас не открываются. Поддомены входят сами.</p>
        </div>
      </div>
      <form class="field-row" onsubmit={submitDomain}>
        <input
          aria-label="Домен сайта"
          placeholder="example.com"
          bind:value={domain}
          maxlength={253}
          required
          autocomplete="off"
          spellcheck="false"
        />
        <button class="btn" type="submit" disabled={!app.installed || app.busy || app.domainBusy || !domain.trim()}>
          <Icon name="plus" size={15} />Добавить
        </button>
      </form>
      {#if app.domains.length}
        <p class="count">
          {app.domains.length}
          {plural(app.domains.length, 'домен', 'домена', 'доменов')} в вашем списке, общий список не меняется
        </p>
        <ul class="domains">
          {#each app.domains as d (d)}
            <li class:fresh={d === app.lastDomain}>
              <span class="dom-name">{d}</span>
              {#if d === app.lastDomain}<span class="fresh-tag">добавлен</span>{/if}
              <button
                class="icon-btn"
                type="button"
                aria-label={`Удалить ${d}`}
                title="Удалить из списка"
                disabled={app.busy || app.domainBusy}
                onclick={() => app.removeDomain(d)}
              >
                <Icon name="close" size={14} />
              </button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="count">Ваш список пока пуст. Общий список при этом работает.</p>
      {/if}
    </section>

    <section class="panel" aria-labelledby="log-title">
      <div class="panel-h">
        <div>
          <h2 class="h2" id="log-title">Журнал службы</h2>
          <p class="sub">Последние 80 строк, только на этом компьютере. Может содержать сетевые адреса.</p>
        </div>
      </div>
      <pre class="log" aria-label="Строки журнала">{app.logLines.join('\n') ||
          (app.installed ? 'Журнал пуст.' : 'Журнал появится, когда служба будет установлена.')}</pre>
      <div class="log-actions">
        <button class="btn sm" disabled={!app.installed} onclick={app.loadLogs}>
          <Icon name="refresh" size={15} />Обновить
        </button>
        <button class="btn sm ghost" disabled={!app.logLines.length} onclick={app.copyLogs}>
          <Icon name="copy" size={15} />Копировать
        </button>
      </div>
    </section>
  </div>
  {#if cropping}
    <IconCropper
      onsave={(png) => {
        appearance.setIcon(png);
        cropping = false;
      }}
      onclose={() => (cropping = false)}
    />
  {/if}
  <footer class="about">
    <Mark size={22} />
    <div>
      <p class="about-t">Zapret Desktop {app.data?.version || '0.1.0'}</p>
      <p class="about-d">Открытый исходный код, лицензия GPLv3. Компоненты движков распространяются по своим лицензиям.</p>
    </div>
  </footer>
</div>

<style>
  .set {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 384px;
    grid-template-areas: 'left right' 'about right';
    grid-template-rows: auto 1fr;
    gap: 16px;
    align-items: start;
  }
  .left {
    grid-area: left;
  }
  .right {
    grid-area: right;
  }
  .about {
    grid-area: about;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }

  .rows .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px 24px;
    padding: 18px 20px;
    border-top: 1px solid var(--line);
  }
  .rows .row:first-child {
    border-top: 0;
  }
  .row-k {
    font-size: 14px;
    font-weight: 500;
  }
  .row-d {
    margin-top: 3px;
    max-width: 380px;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--text-2);
  }
  .value {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-2);
    white-space: nowrap;
  }
  .value.bad {
    color: var(--red);
  }
  .head-row {
    padding-bottom: 6px !important;
  }
  .icon-row {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .icon-preview {
    width: 44px;
    height: 44px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 12px;
    background: var(--raise);
    border: 1px solid var(--line);
  }
  .icon-actions {
    display: flex;
    gap: 6px;
  }
  .row-d.bad {
    color: var(--red);
  }
  .locked {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .select {
    position: relative;
    display: flex;
    align-items: center;
  }
  .select select {
    appearance: none;
    width: 262px;
    max-width: 100%;
    padding-right: 34px;
    text-overflow: ellipsis;
    cursor: pointer;
  }
  .select :global(svg) {
    position: absolute;
    right: 10px;
    color: var(--text-2);
    pointer-events: none;
  }

  .field-row {
    display: flex;
    gap: 8px;
    padding: 0 20px 14px;
  }
  .field-row input {
    flex: 1;
  }
  .count {
    padding: 0 20px 12px;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .domains {
    list-style: none;
    max-height: 264px;
    overflow: auto;
  }
  .domains li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 0 10px 0 20px;
    border-top: 1px solid var(--line);
    font-size: 13.5px;
  }
  .domains li.fresh {
    background: var(--g-soft);
  }
  .dom-name {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .fresh-tag {
    font-size: 12px;
    color: var(--g-text);
  }

  .log {
    margin: 0 20px;
    padding: 12px 14px;
    max-height: 220px;
    overflow: auto;
    border-radius: 10px;
    background: var(--log-bg);
    border: 1px solid var(--line);
    font-family: var(--mono);
    font-size: 11.5px;
    line-height: 1.75;
    color: var(--text-2);
    white-space: pre;
  }
  .log-actions {
    display: flex;
    gap: 8px;
    padding: 14px 20px 18px;
  }

  .about {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 2px 4px 0;
  }
  .about-t {
    font-size: 13px;
    font-weight: 600;
  }
  .about-d {
    margin-top: 2px;
    font-size: 12px;
    color: var(--text-3);
  }

  @media (max-width: 1000px) {
    .set {
      grid-template-columns: minmax(0, 1fr);
      grid-template-areas: 'left' 'right' 'about';
      grid-template-rows: auto;
    }
  }
  @media (max-width: 720px) {
    .rows .row {
      grid-template-columns: minmax(0, 1fr);
    }
    .select select {
      width: 100%;
    }
  }
</style>
