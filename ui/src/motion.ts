/**
 * The window's spring: quick start, a small overshoot, a soft landing. Used
 * by the tab pill.
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
 * A page appearing: opacity only and short. Blur, scale or many moving blocks
 * made switching pages janky, so the spring is kept for the tab pill alone.
 */
export function flow(_node: Element, { delay = 0, duration = 180 } = {}) {
  return {
    delay,
    duration,
    css: (t: number) => `opacity:${Math.min(Math.max(t, 0), 1)}`,
  };
}
