<script lang="ts">
  // Brand mark: "io" with an iolite crystal for the i, on white, or the icon chosen in
  // settings. Same drawing as resources/icon.svg.
  import { appearance } from '../appearance.svelte';

  let { size = 20 }: { size?: number } = $props();
</script>

{#if appearance.icon}
  <img class="mark" src={appearance.icon} width={size} height={size} alt="" />
{:else}
  <svg class="mark" width={size} height={size} viewBox="0 0 1024 1024" aria-hidden="true">
    <rect width="1024" height="1024" rx="232" fill="var(--mark-bg, #ffffff)" />
    <polygon points="330,300 330,396 266,364" fill="#c9bbff"/>
    <polygon points="330,300 394,364 330,396" fill="#a691f6"/>
    <polygon points="266,364 330,396 330,780 266,716" fill="#8a73ea"/>
    <polygon points="330,396 394,364 394,716 330,780" fill="#6a52cf"/>
    <polygon points="330,156 330,198 280,206" fill="#c9bbff"/>
    <polygon points="330,156 380,206 330,198" fill="#a691f6"/>
    <polygon points="280,206 330,198 330,256" fill="#8a73ea"/>
    <polygon points="330,198 380,206 330,256" fill="#6a52cf"/>
    <circle cx="612" cy="600" r="148" fill="none" stroke="#32216b" stroke-width="96"/>
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
