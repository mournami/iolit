<script lang="ts">
  import Icon from '../Icon.svelte';
  import type { ProbeResult } from '../types';

  let {
    running,
    results,
    checking,
    deviceLabel,
    deviceIcon,
  }: {
    running: boolean;
    results: ProbeResult[];
    checking: boolean;
    deviceLabel: string;
    deviceIcon: string;
  } = $props();

  const uid = $props.id();

  const services = [
    { id: 'youtube', name: 'YouTube', hint: 'Открывается главная страница', icon: 'play' },
    { id: 'discord', name: 'Discord', hint: 'Отвечает API', icon: 'chat' },
    { id: 'udp', name: 'UDP', hint: 'Отвечает STUN-сервер', icon: 'wave' },
    { id: 'cloudflare', name: 'Сайты на Cloudflare', hint: 'Проходит HTTPS', icon: 'globe' },
  ];

  // Geometry of the diagram, in SVG units that match CSS pixels 1:1.
  const W = 340;
  const ROW = 52;
  const H = ROW * services.length;
  const MID = H / 2;
  const BARRIER_X = 150;
  const HUB_X = 228;

  type Status = 'ok' | 'fail' | 'wait' | 'none';
  const labels: Record<Status, string> = {
    ok: 'Доступен',
    fail: 'Не подтверждён',
    wait: 'Проверяем…',
    none: 'Не проверен',
  };

  const rows = $derived(
    services.map((s, i) => {
      const result = results.find((r) => r.id === s.id);
      const status: Status = result ? (result.ok ? 'ok' : 'fail') : checking ? 'wait' : 'none';
      return { ...s, y: ROW * i + ROW / 2, result, status };
    }),
  );
  const internet = $derived(results.find((r) => r.id === 'internet'));

  const branch = (y: number) => `M${HUB_X} ${MID}C${HUB_X + 40} ${MID} ${HUB_X + 44} ${y} ${HUB_X + 84} ${y}H${W}`;
  // With the bypass on, traffic is drawn as two lanes that split before the
  // filter and rejoin after it. Both lanes are full-length paths so their
  // dashes stay in phase where they overlap.
  const lane = (dy: number) =>
    `M45 ${MID}H108C121 ${MID} 123 ${MID + dy} 136 ${MID + dy}H190C203 ${MID + dy} 205 ${MID} 218 ${MID}H${HUB_X}`;
</script>

<div class="route" class:on={running} style="--row:{ROW}px">
  <svg class="diagram" viewBox="0 0 {W} {H}" width={W} height={H} aria-hidden="true">
    <defs>
      <pattern id="{uid}-hatch" width="7" height="7" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
        <rect width="2.6" height="7" class="stripe" />
      </pattern>
    </defs>

    {#each rows as row (row.id)}
      <path d={branch(row.y)} class="branch {row.status}" />
    {/each}

    {#if running}
      <rect x={BARRIER_X} y={MID - 74} width="26" height="148" rx="5" class="barrier" fill="url(#{uid}-hatch)" />
      <path d={lane(-9)} class="trunk" />
      <path d={lane(9)} class="trunk" />
    {:else}
      <path d="M45 {MID}H{HUB_X}" class="trunk" />
      <rect x={BARRIER_X} y={MID - 74} width="26" height="148" rx="5" class="barrier" fill="url(#{uid}-hatch)" />
    {/if}

    <circle cx={HUB_X} cy={MID} r="3.5" class="hub" />
    <rect x="1" y={MID - 22} width="44" height="44" rx="11" class="node" />
    <g class="node-icon" transform="translate(13 {MID - 10})"><Icon name={deviceIcon} size={20} /></g>

    <text x="0" y={MID + 44} class="label">{deviceLabel}</text>
    {#if internet}
      <text x="0" y={MID + 62} class="sublabel" class:ok={internet.ok} class:fail={!internet.ok}>
        {internet.ok ? 'Интернет есть' : 'Нет интернета'}
      </text>
    {/if}
    <text x={BARRIER_X + 13} y={H - 6} text-anchor="middle" class="label">Фильтр провайдера</text>
  </svg>

  <ul class="services">
    {#each rows as row (row.id)}
      <li class={row.status} title={row.result?.detail}>
        <span class="svc-icon"><Icon name={row.icon} size={18} /></span>
        <span class="svc-name">
          <strong>{row.name}</strong>
          <small>{row.hint}</small>
        </span>
        <span class="svc-status">
          {labels[row.status]}
          {#if row.result?.ok}<span class="latency">{row.result.latency_ms} мс</span>{/if}
        </span>
      </li>
    {/each}
  </ul>
</div>

<style>
  .route {
    display: flex;
    align-items: stretch;
    min-width: 0;
  }
  .diagram {
    flex: none;
    overflow: visible;
  }

  /* Lines */
  .trunk {
    fill: none;
    stroke: var(--ink-3);
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .on .trunk {
    stroke: var(--signal);
    stroke-dasharray: 9 5;
    animation: flow 0.8s linear infinite;
  }
  .branch {
    fill: none;
    stroke: var(--ink-4);
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-dasharray: 1.5 5;
  }
  .branch.wait {
    stroke: var(--ink-3);
  }
  .branch.ok {
    stroke: var(--signal);
    stroke-width: 2;
    stroke-dasharray: none;
  }
  .branch.fail {
    stroke: var(--caution);
    stroke-width: 2;
    stroke-dasharray: 6 6;
  }
  .hub {
    fill: var(--ink-3);
  }
  .on .hub {
    fill: var(--signal);
  }

  /* Barrier and device */
  .barrier {
    stroke: var(--line-strong);
    stroke-width: 1;
  }
  .stripe {
    fill: var(--ink-4);
  }
  .on .barrier {
    opacity: 0.55;
  }
  .node {
    fill: var(--raised);
    stroke: var(--line-strong);
  }
  .node-icon {
    color: var(--ink);
  }
  .label {
    fill: var(--ink-2);
    font-size: 12px;
  }
  .sublabel {
    font-size: 12px;
    fill: var(--ink-3);
  }
  .sublabel.ok {
    fill: var(--signal);
  }
  .sublabel.fail {
    fill: var(--fault);
  }

  /* Service rows, aligned to the branch ends */
  .services {
    list-style: none;
    margin: 0;
    padding: 0;
    flex: 1;
    min-width: 0;
  }
  .services li {
    height: var(--row);
    display: grid;
    grid-template-columns: 34px minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
  }
  .svc-icon {
    width: 34px;
    height: 34px;
    border-radius: 9px;
    display: grid;
    place-items: center;
    border: 1.5px dashed var(--ink-4);
    color: var(--ink-2);
  }
  .ok .svc-icon {
    border: 1.5px solid var(--signal);
    color: var(--ink);
  }
  .fail .svc-icon {
    border: 1.5px dashed var(--caution);
  }
  .svc-name {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .svc-name strong {
    font-weight: 560;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .svc-name small {
    font-size: 12px;
    color: var(--ink-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .svc-status {
    font-size: 13px;
    color: var(--ink-3);
    white-space: nowrap;
    text-align: right;
  }
  .ok .svc-status {
    color: var(--signal);
  }
  .fail .svc-status {
    color: var(--caution);
  }
  .latency {
    display: inline-block;
    min-width: 4.5em;
    color: var(--ink-2);
    font-variant-numeric: tabular-nums;
  }

  @keyframes flow {
    to {
      stroke-dashoffset: -14;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .on .trunk {
      animation: none;
    }
  }
  @media (max-width: 860px) {
    .svc-name small {
      display: none;
    }
  }
</style>
