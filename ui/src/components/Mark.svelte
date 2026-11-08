<script lang="ts">
  // Brand mark: a hatched barrier with a split line passing through it,
  // the same picture as the route diagram on the overview page.
  let { size = 22 }: { size?: number } = $props();
  const uid = $props.id();
</script>

<svg class="mark" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
  <defs>
    <pattern id="{uid}-hatch" width="3.6" height="3.6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
      <rect width="1.5" height="3.6" fill="currentColor" />
    </pattern>
  </defs>
  <rect x="8.25" y="2.5" width="7.5" height="19" rx="1.75" fill="url(#{uid}-hatch)" opacity="0.8" />
  <path d="M1.75 12H5.5l2.25-3h8.5l2.25 3h3.75M5.5 12l2.25 3h8.5l2.25-3" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
</svg>
