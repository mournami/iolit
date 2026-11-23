<script lang="ts">
  // Minimise, maximise and close for the undecorated Windows window
  // (decorations are off only in tauri.windows.conf.json).
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';

  const win = getCurrentWindow();
  let maximized = $state(false);
  const sizeAction = $derived(maximized ? 'Восстановить размер окна' : 'Развернуть');

  onMount(() => {
    const refresh = () => {
      void win.isMaximized().then((value) => (maximized = value)).catch(console.warn);
    };
    const listener = win.onResized(refresh);
    void listener.then(refresh).catch(console.warn);
    return () => {
      void listener.then((unlisten) => unlisten(), () => {});
    };
  });
</script>

<div class="winctl">
  <button aria-label="Свернуть" title="Свернуть" onclick={() => win.minimize()}>
    <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" stroke="currentColor" stroke-width="1" /></svg>
  </button>
  <button aria-label={sizeAction} title={sizeAction} onclick={() => win.toggleMaximize()}>
    <svg viewBox="0 0 10 10" aria-hidden="true" fill="none" stroke="currentColor">
      {#if maximized}
        <rect x=".5" y="2.5" width="7" height="7" rx="1" />
        <path d="M2.5 2.5v-1a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" />
      {:else}
        <rect x=".5" y=".5" width="9" height="9" rx="1.5" />
      {/if}
    </svg>
  </button>
  <button class="close" aria-label="Закрыть" title="Закрыть" onclick={() => win.close()}>
    <svg viewBox="0 0 10 10" aria-hidden="true"
      ><path d="M.5.5l9 9M9.5.5l-9 9" stroke="currentColor" stroke-width="1.05" /></svg
    >
  </button>
</div>

<style>
  .winctl {
    display: flex;
    align-self: stretch;
    margin: 0 -10px 0 6px;
  }
  button {
    width: 46px;
    border: 0;
    padding: 0;
    background: none;
    display: grid;
    place-items: center;
    color: var(--text-2);
  }
  button:hover {
    background: var(--hover);
    color: var(--text);
  }
  button.close:hover {
    background: #c42b1c;
    color: #ffffff;
  }
  button:focus-visible {
    outline-offset: -2px;
  }
  svg {
    width: 10px;
    height: 10px;
  }
</style>
