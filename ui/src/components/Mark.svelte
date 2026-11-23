<script lang="ts">
  // Brand mark, light or dark with the theme, or the icon chosen in settings.
  import { appearance } from '../appearance.svelte';
  import { markSvg } from '../brandMark';
  import { theme } from '../theme.svelte';

  let { size = 20 }: { size?: number } = $props();
</script>

{#if appearance.icon}
  <img class="mark" src={appearance.icon} width={size} height={size} alt="" />
{:else}
  <span class="mark" style:width="{size}px" style:height="{size}px">{@html markSvg(theme.resolved, size)}</span>
{/if}

<style>
  .mark {
    display: block;
    flex: none;
  }
  .mark :global(svg) {
    display: block;
  }
  img.mark {
    border-radius: 30%;
    object-fit: cover;
  }
</style>
