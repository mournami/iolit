<script lang="ts">
  import { onMount } from 'svelte';
  import { save } from '@tauri-apps/plugin-dialog';
  import {api,native} from './api';import type {Snapshot} from './types';import Icon from './Icon.svelte';
  let page:'overview'|'diagnostics'|'settings'='overview';
  let data:Snapshot|null=null,error='',notice='',pending=false,domain='',domainBusy=false,logLines:string[]=[],showLogs=false,doctorChecks:{name:string;ok:boolean;detail?:string}[]=[];
  let selected='';let refreshing=false;
  $: service=data?.service;
  $: operation=data?.operation;
  $: busy=pending||!!operation?.busy;
  $: connected=!!service?.running;
  $: results=operation?.results||[];
  $: platform=service?.platform==='windows'?'Windows':service?.platform==='macos'?'macOS':'Desktop';
  $: uptime=service?.uptime_secs?`${Math.floor(service.uptime_secs/3600).toString().padStart(2,'0')}:${Math.floor(service.uptime_secs/60%60).toString().padStart(2,'0')}:${Math.floor(service.uptime_secs%60).toString().padStart(2,'0')}`:'—';
  const services=[{id:'youtube',name:'YouTube',hint:'Доступ к сайту',icon:'youtube',color:'#f15b5b'},{id:'discord',name:'Discord',hint:'Соединение с API',icon:'discord',color:'#8b92ff'},{id:'udp',name:'UDP',hint:'Проверка STUN',icon:'pulse',color:'#80cba5'},{id:'cloudflare',name:'Сайты',hint:'Проверка HTTPS',icon:'globe',color:'#ebbd78'}];
  async function refresh(){if(refreshing||!native)return;refreshing=true;try{data=await api<Snapshot>('snapshot');selected=data.service.strategy; }catch(e){error=String(e);}finally{refreshing=false;}}
  onMount(()=>{void refresh();const timer=setInterval(()=>void refresh(),2000);return()=>clearInterval(timer);});
  async function action(command:string,args?:Record<string,unknown>){pending=true;error='';notice='';try{await api(command,args);await refresh();}catch(e){error=String(e);}finally{pending=false;}}
  async function toggle(){await action('control',{action:connected?'disable':'enable'});}
  async function addDomain(){domainBusy=true;error='';try{await api('add_domain',{domain});notice=`Домен ${domain.trim()} добавлен в ваш список`;domain='';}catch(e){error=String(e);}finally{domainBusy=false;}}
  async function checkDoctor(){pending=true;error='';try{const r=await api<{checks:typeof doctorChecks}>('doctor');doctorChecks=r.checks||[];}catch(e){error=String(e);}finally{pending=false;}}
  async function loadLogs(){showLogs=!showLogs;if(showLogs){try{logLines=await api<string[]>('logs');}catch(e){error=String(e);}}}
  async function exportReport(){try{const path=await save({defaultPath:'zapret-diagnostics.json',filters:[{name:'Отчёт JSON',extensions:['json']}]});if(path){await api('export_report',{path});notice='Отчёт сохранён. IP-адреса, домены пользователя и журнал соединений в него не включены.';}}catch(e){error=String(e);}}
</script>

<svelte:head><title>Zapret Desktop</title></svelte:head>
<div class="app-shell">
  <aside class="sidebar">
    <div class="brand"><div class="brand-mark"><Icon name="bolt" size={24}/></div><div><strong>zapret<span>.</span></strong><small>DESKTOP</small></div></div>
    <nav aria-label="Основная навигация">
      {#each [{id:'overview',label:'Обзор',icon:'overview'},{id:'diagnostics',label:'Диагностика',icon:'pulse'},{id:'settings',label:'Настройки',icon:'settings'}] as item}
        <button class:active={page===item.id} onclick={()=>page=item.id as typeof page}><Icon name={item.icon}/>{item.label}{#if page===item.id}<span class="nav-dot"></span>{/if}</button>
      {/each}
    </nav>
    <div class="sidebar-bottom"><div class="local-note"><Icon name="shield" size={18}/><div>Работает на устройстве<small>Ваш трафик не уходит на наш сервер</small></div></div><div class="version"><span>Zapret Desktop</span><span>v{data?.version||'0.1.0'}</span></div></div>
  </aside>
  <main>
    <header><div class="breadcrumb">Рабочее пространство <span>/</span> <strong>{page==='overview'?'Обзор':page==='diagnostics'?'Диагностика':'Настройки'}</strong></div><span class="platform"><span class="tiny-dot"></span>{platform}</span></header>
    <div class="page-body">
      {#if !native}<div class="alert warning"><Icon name="info"/><div><strong>Просмотр интерфейса</strong><p>Для управления сетью откройте установленное приложение Zapret Desktop. Здесь нет имитации подключения.</p></div></div>{/if}
      {#if error}<div class="alert danger" role="alert"><Icon name="info"/><div>{error}</div><button class="icon-button" aria-label="Закрыть ошибку" onclick={()=>error=''}><Icon name="close" size={16}/></button></div>{/if}
      {#if notice}<div class="alert success" role="status"><Icon name="check"/><div>{notice}</div><button class="icon-button" aria-label="Закрыть уведомление" onclick={()=>notice=''}><Icon name="close" size={16}/></button></div>{/if}

      {#if page==='overview'}
        <section class="page-title"><div><div class="eyebrow">ВАША СЕТЬ, БЕЗ ЛИШНИХ ШАГОВ</div><h1>Свободное соединение<span>.</span></h1><p>Один раз настройте. Дальше — одна кнопка.</p></div></section>
        <section class="connection-card" class:connected>
          <div class="connection-content"><div class="status-pill" class:on={connected}><span></span>{connected?'Движок запущен':service?.installed?'Готов к подключению':'Требуется установка службы'}</div><h2>{connected?'Вы подключены':service?.installed?'Всё под вашим контролем':'Начнём с установки'}</h2><p>{connected?'Проверка сервисов покажет, какие соединения доступны в вашей сети.':service?.installed?'Включите обход или подберите стратегию для своей сети.':'Для работы с сетевыми пакетами нужна системная служба. Установка выполняется один раз.'}</p>
            <div class="connection-actions">{#if service?.installed}<button class="button primary" class:disconnect={connected} disabled={busy} onclick={toggle}><Icon name="power" size={19}/>{pending?'Подождите…':connected?'Отключить':'Включить обход'}</button><button class="text-button" disabled={busy} onclick={()=>{page='diagnostics';void action('select_strategy');}}>Подобрать автоматически <Icon name="arrow" size={17}/></button>{:else}<button class="button primary" disabled={!native||pending} onclick={()=>action('install_service')}><Icon name="download" size={19}/>Установить службу</button><button class="text-button" disabled={!native||pending} onclick={refresh}>Проверить установку <Icon name="refresh" size={17}/></button>{/if}</div>
          </div>
          <div class="connection-visual" aria-hidden="true"><div class="orbit orbit-outer"></div><div class="orbit orbit-inner"></div><div class="shield-core"><Icon name={connected?'check':'shield'} size={52}/></div><span class="orbit-point point-one"></span><span class="orbit-point point-two"></span></div>
          <div class="connection-meta"><div><span>Движок</span><strong>{service?.engine||'—'}</strong></div><div><span>Стратегия</span><strong>{service?.strategy||'Пока не выбрана'}</strong></div><div><span>Время работы</span><strong class="mono">{uptime}</strong></div></div>
        </section>
        <section class="services-section"><div class="section-header"><h3>Доступность сервисов</h3><button class="text-button" disabled={!native||busy} onclick={()=>action('diagnose')}><Icon name="refresh" size={15}/>{operation?.busy?'Проверяем…':'Проверить'}</button></div><div class="services-grid">{#each services as s}{@const result=results.find(r=>r.id===s.id)}<article class="service-card"><div class="service-top"><div class="service-icon" style={`--service-color:${s.color}`}><Icon name={s.icon} size={23}/></div><span class="service-state" class:ok={result?.ok} class:failed={result&&!result.ok}>{result?result.ok?'Доступен':'Не подтверждён':'Не проверен'}</span></div><h4>{s.name}</h4><div class="service-bottom"><span>{s.hint}</span><span class="mono">{result?`${result.latency_ms} мс`:'—'}</span></div></article>{/each}</div></section>
        <div class="bottom-grid"><section class="small-panel"><div class="panel-symbol"><Icon name="bolt"/></div><div><h4>Настройка под вашу сеть</h4><p>Подбор проверяет стратегии по очереди и сохраняет ту, которая прошла проверки.</p></div><button class="icon-button" aria-label="Перейти к подбору стратегии" onclick={()=>page='diagnostics'}><Icon name="chevron"/></button></section><section class="small-panel"><div class="panel-symbol green"><Icon name="shield"/></div><div><h4>Без сбора истории</h4><p>Телеметрия не отправляется. Диагностический отчёт сохраняется только по вашему запросу.</p></div></section></div>
        {#if service?.warnings.length}<div class="warnings">{#each service.warnings as warning}<div><Icon name="info" size={16}/><span>{warning}</span></div>{/each}</div>{/if}
      {:else if page==='diagnostics'}
        <section class="page-title"><div><div class="eyebrow">ПОНИМАТЬ, ЧТО ПРОИСХОДИТ</div><h1>Диагностика<span>.</span></h1><p>Проверьте соединения и найдите подходящую стратегию.</p></div><button class="button secondary" disabled={!native} onclick={exportReport}><Icon name="download" size={17}/>Сохранить отчёт</button></section>
        <div class="diagnostic-controls"><button class="button primary" disabled={!native||busy} onclick={()=>action('diagnose')}><Icon name="pulse" size={18}/>Проверить соединения</button><button class="button secondary" disabled={!service?.installed||busy} onclick={()=>action('select_strategy')}><Icon name="bolt" size={18}/>Подобрать стратегию</button>{#if operation?.busy}<button class="text-button" onclick={()=>action('cancel_operation')}>Отменить</button>{/if}</div>
        {#if operation?.busy||operation?.stage}<section class="operation-panel"><div><span class:spinning={operation.busy}><Icon name={operation.busy?'refresh':'check'} size={18}/></span><strong>{operation.stage}</strong>{#if operation.total}<span>{operation.current} / {operation.total}</span>{/if}</div>{#if operation.total}<progress value={operation.current} max={operation.total}></progress>{/if}{#if operation.error}<p class="error-text">{operation.error}</p>{/if}{#if operation.notice}<p class="success-text">{operation.notice}</p>{/if}{#if operation.winner}<p class="success-text">Прошла две серии проверок: {operation.winner}. Воспроизведение видео и голосовой сеанс проверьте в самих приложениях.</p>{/if}</section>{/if}
        <section class="panel results-panel"><div class="section-header"><h3>Результаты проверок</h3><span class="muted">TCP + UDP</span></div>{#if results.length}{#each results as r}<div class="result-row"><div class="result-indicator" class:ok={r.ok}><Icon name={r.ok?'check':'info'} size={18}/></div><div><strong>{r.name}</strong><p>{r.detail}</p></div><span class="protocol">{r.protocol}</span><span class="mono">{r.latency_ms} мс</span></div>{/each}{:else}<div class="empty-state"><Icon name="pulse" size={34}/><h4>Посмотрим, как работает сеть</h4><p>Запустите проверку. Результаты появятся здесь.</p></div>{/if}</section>
        <section class="panel doctor-panel"><div class="section-header"><div><h3>Проверка системы</h3><p>Состояние службы, движка и сетевых правил.</p></div><button class="button secondary" disabled={!service?.installed||busy} onclick={checkDoctor}>Проверить систему</button></div>{#each doctorChecks as check}<div class="check-row"><Icon name={check.ok?'check':'info'} size={17}/><strong>{check.name}</strong><span>{check.detail||'—'}</span></div>{/each}</section>
        <div class="inline-note"><Icon name="info" size={17}/><p>Успешная проверка HTTPS или STUN подтверждает конкретное соединение. Она не подтверждает воспроизведение видео, голос Discord или работу любого другого приложения.</p></div>
      {:else}
        <section class="page-title"><div><div class="eyebrow">ПРОСТОЕ УПРАВЛЕНИЕ</div><h1>Настройки<span>.</span></h1><p>Стратегии, ваши домены и состояние приложения.</p></div></section>
        <section class="panel"><div class="setting-row"><div><h3>Рабочая стратегия</h3><p>Список зависит от установленного движка.</p></div><select aria-label="Стратегия обхода" bind:value={selected} disabled={!service?.installed||busy} onchange={()=>action('control',{action:'strategy',strategyId:selected})}><option value="" disabled>Выберите стратегию</option>{#each data?.strategies||[] as s}<option value={s.id} disabled={!s.supported}>{s.name}{s.supported?'':' · несовместима'}</option>{/each}</select></div><div class="setting-row"><div><h3>После перезапуска компьютера</h3><p>Служба восстанавливает последнее выбранное состояние. Отключение в приложении сохраняется.</p></div><span class="tag">Управляется кнопкой подключения</span></div><div class="setting-row"><div><h3>Телеметрия</h3><p>Приложение не отправляет статистику, историю и диагностические отчёты.</p></div><span class="tag green">Выключена</span></div></section>
        <section class="panel"><h3>Добавить свой домен</h3><p class="setting-description">Добавляется в ваш отдельный список. Данные upstream остаются нетронутыми.</p><form class="domain-form" onsubmit={(e)=>{e.preventDefault();void addDomain();}}><input aria-label="Домен сайта" placeholder="example.com" bind:value={domain} maxlength={253} required autocomplete="off" spellcheck="false"/><button class="button secondary" type="submit" disabled={!service?.installed||busy||domainBusy||!domain.trim()}><Icon name="plus" size={17}/>Добавить</button></form></section>
        <section class="panel"><div class="section-header"><div><h3>Журнал службы</h3><p>Журнал остаётся на устройстве и может содержать сетевые адреса.</p></div><button class="button secondary" disabled={!service?.installed} onclick={loadLogs}><Icon name="terminal" size={17}/>{showLogs?'Скрыть':'Открыть'}</button></div>{#if showLogs}<pre class="log-view">{logLines.join('\n')||'Журнал пуст.'}</pre>{/if}</section>
        <section class="about-panel"><div class="brand-mark small"><Icon name="bolt" size={20}/></div><div><strong>Zapret Desktop <span class="muted">v{data?.version||'0.1.0'}</span></strong><p>Открытый исходный код · MIT. Компоненты движков распространяются по собственным лицензиям.</p></div></section>
      {/if}
    </div>
    <footer><span><span class="tiny-dot" class:on={service?.installed}></span>{service?.installed?'Системная служба доступна':native?'Ожидаем системную службу':'Просмотр интерфейса'}</span><span>Локальный обход DPI</span></footer>
  </main>
</div>
