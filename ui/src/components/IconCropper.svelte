<script lang="ts">
  import { ICON_SIZE } from '../appearance.svelte';
  import Icon from '../Icon.svelte';

  let { onsave, onclose }: { onsave: (png: string) => void; onclose: () => void } = $props();

  /** Large photos are fine; this only stops files that would hang the window. */
  const MAX_BYTES = 25 * 1024 * 1024;
  const VIEW = 300; // viewport, px
  const BOX = 236; // crop square inside it, px
  const PAD = (VIEW - BOX) / 2;

  let dialog: HTMLDialogElement;
  let input: HTMLInputElement;
  let img = $state<HTMLImageElement | null>(null);
  let error = $state('');
  let dragging = $state(false);
  // Image placement: screen px per image px, and the image's top-left
  // relative to the crop square.
  let scale = $state(1);
  let x = $state(0);
  let y = $state(0);
  let minScale = $state(1);
  let preview = $state<HTMLCanvasElement>();
  let previewSmall = $state<HTMLCanvasElement>();

  $effect(() => {
    dialog.showModal();
  });

  function clamp() {
    if (!img) return;
    const w = img.naturalWidth * scale;
    const h = img.naturalHeight * scale;
    x = Math.min(0, Math.max(BOX - w, x));
    y = Math.min(0, Math.max(BOX - h, y));
  }

  function load(file: File | undefined) {
    error = '';
    if (!file) return;
    if (!file.type.startsWith('image/')) {
      error = 'Это не картинка. Подойдут PNG, JPG, WebP, GIF или SVG.';
      return;
    }
    if (file.size > MAX_BYTES) {
      error = 'Файл больше 25 МБ. Уменьшите картинку и попробуйте снова.';
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      const next = new Image();
      next.onload = () => {
        if (!next.naturalWidth || !next.naturalHeight) {
          error = 'Не удалось прочитать размер картинки.';
          return;
        }
        minScale = BOX / Math.min(next.naturalWidth, next.naturalHeight);
        scale = minScale;
        x = (BOX - next.naturalWidth * scale) / 2;
        y = (BOX - next.naturalHeight * scale) / 2;
        img = next;
      };
      next.onerror = () => (error = 'Картинка повреждена или формат не поддерживается.');
      next.src = reader.result as string;
    };
    reader.onerror = () => (error = 'Не удалось открыть файл.');
    reader.readAsDataURL(file);
  }

  /** Zoom keeping the centre of the crop square in place. */
  function zoomTo(next: number) {
    next = Math.min(minScale * 8, Math.max(minScale, next));
    const cx = (BOX / 2 - x) / scale;
    const cy = (BOX / 2 - y) / scale;
    scale = next;
    x = BOX / 2 - cx * scale;
    y = BOX / 2 - cy * scale;
    clamp();
  }

  function pan(e: PointerEvent) {
    if (!img) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    let last = { x: e.clientX, y: e.clientY };
    dragging = true;
    el.onpointermove = (ev) => {
      x += ev.clientX - last.x;
      y += ev.clientY - last.y;
      last = { x: ev.clientX, y: ev.clientY };
      clamp();
    };
    el.onpointerup = () => {
      el.onpointermove = el.onpointerup = null;
      dragging = false;
    };
  }

  function keys(e: KeyboardEvent) {
    if (!img) return;
    const step = e.shiftKey ? 20 : 4;
    const moves: Record<string, [number, number]> = { ArrowLeft: [step, 0], ArrowRight: [-step, 0], ArrowUp: [0, step], ArrowDown: [0, -step] };
    if (moves[e.key]) {
      x += moves[e.key][0];
      y += moves[e.key][1];
      clamp();
    } else if (e.key === '+' || e.key === '=') zoomTo(scale * 1.1);
    else if (e.key === '-') zoomTo(scale / 1.1);
    else return;
    e.preventDefault();
  }

  function draw(canvas: HTMLCanvasElement, size: number) {
    if (!img) return;
    canvas.width = size;
    canvas.height = size;
    const ctx = canvas.getContext('2d')!;
    ctx.imageSmoothingQuality = 'high';
    ctx.clearRect(0, 0, size, size);
    ctx.drawImage(img, -x / scale, -y / scale, BOX / scale, BOX / scale, 0, 0, size, size);
  }

  $effect(() => {
    void [img, scale, x, y];
    if (preview) draw(preview, 96);
    if (previewSmall) draw(previewSmall, 40);
  });

  function save() {
    const canvas = document.createElement('canvas');
    draw(canvas, ICON_SIZE);
    onsave(canvas.toDataURL('image/png'));
  }

  const zoomPercent = $derived(Math.round((scale / minScale) * 100));
</script>

<dialog
  bind:this={dialog}
  class="crop"
  aria-labelledby="crop-title"
  onclose={onclose}
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
>
  <header class="head">
    <h2 class="h2" id="crop-title">Значок в шапке</h2>
    <button class="icon-btn" aria-label="Закрыть" onclick={onclose}><Icon name="close" size={16} /></button>
  </header>

  <input
    bind:this={input}
    type="file"
    accept="image/png,image/jpeg,image/webp,image/gif,image/svg+xml,image/bmp"
    hidden
    onchange={(e) => load((e.currentTarget as HTMLInputElement).files?.[0])}
  />

  {#if !img}
    <button
      class="drop"
      class:over={dragging}
      ondragover={(e) => {
        e.preventDefault();
        dragging = true;
      }}
      ondragleave={() => (dragging = false)}
      ondrop={(e) => {
        e.preventDefault();
        dragging = false;
        load(e.dataTransfer?.files[0]);
      }}
      onclick={() => input.click()}
    >
      <span class="drop-ico"><Icon name="upload" size={22} /></span>
      <span class="drop-t">Перетащите картинку сюда</span>
      <span class="drop-d">или нажмите, чтобы выбрать файл. PNG, JPG, WebP, GIF или SVG любого размера до 25 МБ.</span>
    </button>
  {:else}
    <div class="editor">
      <!-- A pan-and-zoom surface: keyboard users move it with arrows and +/-. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
      <div
        class="view"
        class:grabbing={dragging}
        role="application"
        tabindex="0"
        aria-label="Область обрезки. Стрелки двигают картинку, плюс и минус меняют масштаб."
        style:width={`${VIEW}px`}
        style:height={`${VIEW}px`}
        onpointerdown={pan}
        onwheel={(e) => {
          e.preventDefault();
          zoomTo(scale * (e.deltaY < 0 ? 1.08 : 1 / 1.08));
        }}
        onkeydown={keys}
      >
        <img
          src={img.src}
          alt=""
          draggable="false"
          style:left={`${PAD + x}px`}
          style:top={`${PAD + y}px`}
          style:width={`${img.naturalWidth * scale}px`}
          style:height={`${img.naturalHeight * scale}px`}
        />
        <span class="mask" style:inset={`${PAD}px`}></span>
      </div>

      <div class="side">
        <p class="label">Так будет в шапке</p>
        <div class="titlebar-sample">
          <canvas bind:this={previewSmall} class="ic-small"></canvas>
          <span>iolit</span>
        </div>
        <canvas bind:this={preview} class="ic-big"></canvas>
        <p class="hint">Сохраняется квадрат {ICON_SIZE}×{ICON_SIZE}. Углы скругляются, как у стандартного значка.</p>
      </div>
    </div>

    <div class="zoom">
      <button class="icon-btn" aria-label="Уменьшить" onclick={() => zoomTo(scale / 1.15)}><Icon name="minus" size={16} /></button>
      <input
        type="range"
        min="100"
        max="800"
        value={zoomPercent}
        aria-label="Масштаб"
        oninput={(e) => zoomTo((minScale * Number((e.currentTarget as HTMLInputElement).value)) / 100)}
      />
      <button class="icon-btn" aria-label="Увеличить" onclick={() => zoomTo(scale * 1.15)}><Icon name="plus" size={16} /></button>
      <span class="num pct">{zoomPercent}%</span>
    </div>
  {/if}

  {#if error}<p class="err" role="alert"><Icon name="alert" size={15} />{error}</p>{/if}

  <footer class="foot">
    {#if img}
      <button class="btn ghost" onclick={() => input.click()}><Icon name="image" size={15} />Другая картинка</button>
    {/if}
    <span class="grow"></span>
    <button class="btn ghost" onclick={onclose}>Отмена</button>
    <button class="btn primary" disabled={!img} onclick={save}>Сохранить</button>
  </footer>
</dialog>

<style>
  .crop {
    width: min(560px, calc(100vw - 32px));
    padding: 0;
    border: 1px solid var(--line-2);
    border-radius: 16px;
    background: var(--panel);
    color: var(--text);
    box-shadow: 0 30px 80px rgb(0 0 0 / 0.55);
  }
  .crop::backdrop {
    background: rgb(0 0 0 / 0.6);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 16px 12px 20px;
  }

  .drop {
    margin: 0 20px;
    width: calc(100% - 40px);
    min-height: 240px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 24px;
    border-radius: 12px;
    border: 1.5px dashed var(--line-2);
    background: var(--raise);
    color: var(--text);
    text-align: center;
  }
  .drop:hover,
  .drop.over {
    border-color: var(--g);
    background: var(--g-soft);
  }
  .drop-ico {
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border-radius: 12px;
    background: var(--raise-2);
    color: var(--g-text);
    margin-bottom: 4px;
  }
  .drop-t {
    font-size: 14px;
    font-weight: 500;
  }
  .drop-d {
    max-width: 320px;
    font-size: 12.5px;
    color: var(--text-2);
  }

  .editor {
    display: flex;
    gap: 20px;
    padding: 0 20px;
    align-items: flex-start;
  }
  .view {
    position: relative;
    flex: none;
    overflow: hidden;
    border-radius: 12px;
    background: repeating-conic-gradient(var(--raise) 0 25%, var(--raise-2) 0 50%) 0 0 / 16px 16px;
    cursor: grab;
    touch-action: none;
  }
  .view.grabbing {
    cursor: grabbing;
  }
  .view img {
    position: absolute;
    max-width: none;
    user-select: none;
    pointer-events: none;
  }
  /* Dims everything outside the crop square and draws its rounded outline. */
  .mask {
    position: absolute;
    border-radius: 30%;
    box-shadow: 0 0 0 999px rgb(0 0 0 / 0.55);
    outline: 2px solid rgb(255 255 255 / 0.9);
    pointer-events: none;
  }
  .side {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .label {
    font-size: 12px;
    color: var(--text-3);
  }
  .titlebar-sample {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--bg);
    border: 1px solid var(--line);
    font-size: 15.5px;
    font-weight: 600;
    letter-spacing: -0.02em;
  }
  .ic-small {
    width: 20px;
    height: 20px;
    border-radius: 30%;
  }
  .ic-big {
    width: 96px;
    height: 96px;
    border-radius: 30%;
  }
  .hint {
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-2);
  }

  .zoom {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 20px 0;
  }
  .zoom input {
    flex: 1;
    accent-color: var(--g);
    padding: 0;
    height: auto;
    border: 0;
    background: none;
  }
  .pct {
    width: 44px;
    text-align: right;
    font-size: 12px;
    color: var(--text-2);
  }
  .err {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 12px 20px 0;
    font-size: 12.5px;
    color: var(--red);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 18px 20px 20px;
  }
  .grow {
    flex: 1;
  }

  @media (max-width: 600px) {
    .editor {
      flex-direction: column;
      align-items: center;
    }
  }
</style>
