/** Small colour helpers for the appearance settings. Colours are #rrggbb. */
export type RGB = { r: number; g: number; b: number };
export type HSV = { h: number; s: number; v: number };

const clamp = (x: number, lo = 0, hi = 1) => Math.min(hi, Math.max(lo, x));

/** Accepts #rgb, #rrggbb, with or without '#'; returns #rrggbb or null. */
export function normalizeHex(input: string): string | null {
  let s = input.trim().replace(/^#/, '').toLowerCase();
  if (/^[0-9a-f]{3}$/.test(s)) s = [...s].map((c) => c + c).join('');
  return /^[0-9a-f]{6}$/.test(s) ? `#${s}` : null;
}

export function hexToRgb(hex: string): RGB {
  const n = parseInt(hex.slice(1), 16);
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
}

export function rgbToHex({ r, g, b }: RGB): string {
  return '#' + [r, g, b].map((x) => Math.round(clamp(x, 0, 255)).toString(16).padStart(2, '0')).join('');
}

export function hexToHsv(hex: string): HSV {
  const { r, g, b } = hexToRgb(hex);
  const [R, G, B] = [r / 255, g / 255, b / 255];
  const max = Math.max(R, G, B);
  const d = max - Math.min(R, G, B);
  let h = 0;
  if (d) {
    if (max === R) h = ((G - B) / d) % 6;
    else if (max === G) h = (B - R) / d + 2;
    else h = (R - G) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  return { h, s: max ? d / max : 0, v: max };
}

export function hsvToHex({ h, s, v }: HSV): string {
  const c = v * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = v - c;
  const [r, g, b] =
    h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return rgbToHex({ r: (r + m) * 255, g: (g + m) * 255, b: (b + m) * 255 });
}

/** Linear mix: t = 0 gives a, t = 1 gives b. */
export function mix(a: string, b: string, t: number): string {
  const A = hexToRgb(a);
  const B = hexToRgb(b);
  return rgbToHex({ r: A.r + (B.r - A.r) * t, g: A.g + (B.g - A.g) * t, b: A.b + (B.b - A.b) * t });
}

export function alpha(hex: string, a: number): string {
  const { r, g, b } = hexToRgb(hex);
  return `rgb(${r} ${g} ${b} / ${a})`;
}

/** WCAG relative luminance. */
export function luminance(hex: string): number {
  const f = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  const { r, g, b } = hexToRgb(hex);
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

export function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

export const isLight = (hex: string) => luminance(hex) > 0.36;

/** Moves `color` towards white or black until it reads on `bg` (WCAG AA). */
export function readableOn(color: string, bg: string, target = 4.5): string {
  const towards = isLight(bg) ? '#000000' : '#ffffff';
  for (let t = 0; t <= 1; t += 0.04) {
    const c = mix(color, towards, t);
    if (contrast(c, bg) >= target) return c;
  }
  return towards;
}

/** Surface, line and text tokens derived from a background colour. */
export function surfaceTokens(bg: string): Record<string, string> {
  const light = isLight(bg);
  const text = light ? '#1a1433' : '#f1eefb';
  if (light) {
    return {
      '--bg': bg,
      '--panel': mix(bg, text, 0.04),
      '--raise': mix(bg, '#ffffff', 0.6),
      '--raise-2': mix(bg, text, 0.085),
      '--tab-on': mix(bg, text, 0.07),
      '--line': mix(bg, text, 0.1),
      '--line-2': mix(bg, text, 0.17),
      '--text': text,
      '--text-2': mix(text, bg, 0.4),
      '--text-3': mix(text, bg, 0.6),
      '--slab-off': mix(bg, text, 0.06),
      '--log-bg': mix(bg, '#ffffff', 0.6),
      '--hover': alpha(text, 0.05),
    };
  }
  return {
    '--bg': bg,
    '--panel': mix(bg, text, 0.045),
    '--raise': mix(bg, text, 0.08),
    '--raise-2': mix(bg, text, 0.11),
    '--tab-on': mix(bg, text, 0.095),
    '--line': mix(bg, text, 0.11),
    '--line-2': mix(bg, text, 0.165),
    '--text': text,
    '--text-2': mix(text, bg, 0.38),
    '--text-3': mix(text, bg, 0.62),
    '--slab-off': mix(bg, text, 0.065),
    '--log-bg': mix(bg, text, 0.025),
    '--hover': alpha('#ffffff', 0.05),
  };
}

/** Accent tokens: the fill, readable text in that colour, soft tint, text on the fill. */
export function accentTokens(accent: string, bg: string): Record<string, string> {
  return {
    '--g': accent,
    '--g-text': readableOn(accent, bg),
    '--g-soft': alpha(accent, 0.11),
    '--on-g': contrast(accent, '#1a1040') >= contrast(accent, '#ffffff') ? '#1a1040' : '#ffffff',
  };
}
