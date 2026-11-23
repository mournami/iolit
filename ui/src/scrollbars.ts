import { OverlayScrollbars } from 'overlayscrollbars';

export function scrollbars(host: HTMLElement, { horizontal = false } = {}) {
  const viewport = host.firstElementChild;
  if (!(viewport instanceof HTMLElement)) return;

  const forcedColors = matchMedia('(forced-colors: active)');
  let instance: OverlayScrollbars | undefined;

  const update = () => {
    instance?.destroy();
    instance = forcedColors.matches ? undefined : OverlayScrollbars(
      // Keep the viewport's layout styles and mount the handles beside it.
      { target: viewport, elements: { viewport }, scrollbars: { slot: host } },
      {
        overflow: { x: horizontal ? 'scroll' : 'hidden' },
        scrollbars: { theme: 'os-theme-iolit', clickScroll: 'instant' },
      },
    );
  };

  update();
  forcedColors.addEventListener('change', update);
  return {
    destroy() {
      forcedColors.removeEventListener('change', update);
      instance?.destroy();
    },
  };
}
