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

/** Page enter and leave: blur, scale and fade. Blocks do the sliding themselves. */
export function flow(_node: Element, { delay = 0, duration = 520, leaving = false } = {}) {
  return {
    delay,
    duration,
    easing: leaving ? (t: number) => t : spring,
    css: (t: number) => {
      const shown = Math.min(Math.max(t, 0), 1);
      const hidden = 1 - shown;
      return `opacity:${shown};transform:scale(${1 - 0.012 * hidden});filter:blur(${Math.max(0, hidden * (leaving ? 4 : 8)).toFixed(2)}px)`;
    },
  };
}
