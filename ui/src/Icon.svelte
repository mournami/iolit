<script lang="ts">
  let { name = 'info', size = 20 }: { name?: string; size?: number } = $props();

  const paths: Record<string, string> = {
    check: 'm5 12.5 4.5 4.5L19 7',
    close: 'M6 6l12 12M18 6 6 18',
    alert: 'M12 7.5v5.5M12 16.5v.01M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z',
    info: 'M12 11v5.5M12 7.5v.01M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z',
    download: 'M12 4v11m-4.5-4.5L12 15l4.5-4.5M5 19.5h14',
    refresh: 'M19.5 10A8 8 0 0 0 5.2 7.6L4 9M4 4.5V9h4.5M4.5 14a8 8 0 0 0 14.3 2.4L20 15M20 19.5V15h-4.5',
    pulse: 'M3 12h4l2.5-6 5 12 2.5-6h4',
    sliders: 'M4 7h9M17 7h3M4 17h3M11 17h9M15 5v4M9 15v4',
    plus: 'M12 5v14M5 12h14',
    terminal: 'M4.5 5h15a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1h-15a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1ZM8 10l2.5 2L8 14M13 14h3',
    play: 'M5 6h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2ZM10 9.5v5l4.3-2.5L10 9.5Z',
    chat: 'M5 5h14a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1h-8l-4 3.5V16H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1Z',
    wave: 'M3.5 12h1M7.5 8.5v7M11.5 5v14M15.5 9v6M19.5 11v2',
    globe: 'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0ZM3.5 9h17M3.5 15h17M12 3c2.4 2.5 3.5 5.5 3.5 9s-1.1 6.5-3.5 9c-2.4-2.5-3.5-5.5-3.5-9s1.1-6.5 3.5-9Z',
    laptop: 'M5 5.5h14a1 1 0 0 1 1 1V15H4V6.5a1 1 0 0 1 1-1ZM2.5 18.5h19',
    monitor: 'M4 4h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1ZM8 20h8M12 16v4',
  };
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={paths[name] || paths.info} /></svg>
