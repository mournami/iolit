<script lang="ts">
  // Brand mark: a round-cut iolite on the brand violet, or the icon chosen in
  // settings. Same drawing as resources/icon.svg.
  import { appearance } from '../appearance.svelte';

  let { size = 20 }: { size?: number } = $props();
</script>

{#if appearance.icon}
  <img class="mark" src={appearance.icon} width={size} height={size} alt="" />
{:else}
  <svg class="mark" width={size} height={size} viewBox="0 0 1024 1024" aria-hidden="true">
    <rect width="1024" height="1024" rx="232" fill="var(--mark-bg, #32216b)" />
    <circle cx="512" cy="512" r="300" fill="#a893f7"/>
    <polygon points="569.4,373.4 724.1,299.9 650.6,454.6" fill="#c9bcff"/>
    <polygon points="650.6,454.6 812.0,512.0 650.6,569.4" fill="#8f78ec"/>
    <polygon points="650.6,569.4 724.1,724.1 569.4,650.6" fill="#c9bcff"/>
    <polygon points="569.4,650.6 512.0,812.0 454.6,650.6" fill="#8f78ec"/>
    <polygon points="454.6,650.6 299.9,724.1 373.4,569.4" fill="#c9bcff"/>
    <polygon points="373.4,569.4 212.0,512.0 373.4,454.6" fill="#8f78ec"/>
    <polygon points="373.4,454.6 299.9,299.9 454.6,373.4" fill="#c9bcff"/>
    <polygon points="454.6,373.4 512.0,212.0 569.4,373.4" fill="#8f78ec"/>
    <polygon points="569.4,373.4 650.6,454.6 650.6,569.4 569.4,650.6 454.6,650.6 373.4,569.4 373.4,454.6 454.6,373.4" fill="#e6dfff"/>
    <circle cx="512" cy="512" r="300" fill="none" stroke="#1d1240" stroke-opacity="0.35" stroke-width="10"/>
  </svg>
{/if}

<style>
  .mark {
    display: block;
    flex: none;
  }
  img.mark {
    border-radius: 30%;
    object-fit: cover;
  }
</style>
