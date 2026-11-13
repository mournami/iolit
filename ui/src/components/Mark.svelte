<script lang="ts">
  // Brand mark: a green tile with a "z".
  let { size = 20 }: { size?: number } = $props();
</script>

<svg class="mark" width={size} height={size} viewBox="0 0 20 20" aria-hidden="true">
  <rect width="20" height="20" rx="6" fill="var(--g)" />
  <path
    d="M5.5 6.2h9l-9 7.6h9"
    fill="none"
    stroke="var(--on-g)"
    stroke-width="2.1"
    stroke-linecap="round"
    stroke-linejoin="round"
  />
</svg>
