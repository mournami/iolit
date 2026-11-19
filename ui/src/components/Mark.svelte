<script lang="ts">
  // Brand mark: a green tile with a "z", or the icon chosen in settings.
  import { appearance } from '../appearance.svelte';

  let { size = 20 }: { size?: number } = $props();
</script>

{#if appearance.icon}
  <img class="mark" src={appearance.icon} width={size} height={size} alt="" />
{:else}
  <svg class="mark" width={size} height={size} viewBox="0 0 20 20" aria-hidden="true">
    <rect width="20" height="20" rx="6" fill="var(--mark-bg, var(--g))" />
    <path
      d="M5.5 6.2h9l-9 7.6h9"
      fill="none"
      stroke="var(--mark-fg, var(--on-g))"
      stroke-width="2.1"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
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
