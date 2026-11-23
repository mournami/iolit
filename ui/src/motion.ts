/**
 * One spring for all movement in the window, so the tab pill, the page and
 * its blocks feel like the same material: quick start, a small overshoot,
 * a soft landing.
 */

/** Damping ratio and settle point: the spring is within 1% at t = 1. */
const ZETA = 0.72;
const OMEGA = 4.6 / ZETA;

/** Spring position for t in 0..1; slightly above 1 on the way, exactly 1 at the end. */
export function spring(t: number): number {
  if (t <= 0) return 0;
  if (t >= 1) return 1;
  const wd = OMEGA * Math.sqrt(1 - ZETA * ZETA);
  const decay = Math.exp(-ZETA * OMEGA * t);
  return 1 - decay * (Math.cos(wd * t) + ((ZETA * OMEGA) / wd) * Math.sin(wd * t));
}

/** The same curve as a CSS `linear()` easing, for transitions and animations. */
export const SPRING_CSS = `linear(${Array.from({ length: 41 }, (_, i) => spring(i / 40).toFixed(4)).join(', ')})`;

/**
 * Page enter and leave: opacity only. Blur or scale on a whole page makes the
 * browser repaint every pixel on each frame, which janks; the blocks of the
 * page do the moving (see App.svelte), and they only use transform and opacity.
 */
export function flow(_node: Element, { delay = 0, duration = 360, leaving = false } = {}) {
  return {
    delay,
    duration,
    easing: leaving ? (t: number) => t : (t: number) => Math.min(spring(t), 1),
    css: (t: number) => `opacity:${Math.min(Math.max(t, 0), 1)}`,
  };
}
