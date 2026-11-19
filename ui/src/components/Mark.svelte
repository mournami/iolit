<script lang="ts">
  // Brand mark: "io" with a cut iolite crystal for the dot of the i, or the icon chosen in
  // settings. Same drawing as resources/icon.svg.
  import { appearance } from '../appearance.svelte';

  let { size = 20 }: { size?: number } = $props();
</script>

{#if appearance.icon}
  <img class="mark" src={appearance.icon} width={size} height={size} alt="" />
{:else}
  <svg class="mark" width={size} height={size} viewBox="0 0 1024 1024" aria-hidden="true">
    <rect width="1024" height="1024" rx="232" fill="var(--mark-bg, #32216b)" />
    <rect x="282" y="448" width="96" height="312" rx="48" fill="#f1eefb"/>
    <polygon points="330,196 388,254 388,346 330,404 272,346 272,254" fill="#a893f7"/>
    <polygon points="330,196 388,254 330,285.9" fill="#e2d9ff"/>
    <polygon points="330,196 330,285.9 272,254" fill="#c9bcff"/>
    <polygon points="330,285.9 388,254 388,346 330,404" fill="#8670e3"/>
    <circle cx="612" cy="600" r="148" fill="none" stroke="#f1eefb" stroke-width="96"/>
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
