/**
 * The Iolit mark: an iolite crystal for the i and a ring for the o. The light
 * version (white tile, violet o) is the exe icon and resources/icon.svg; the
 * dark one (violet tile, light o) is used with the dark theme.
 */
const CRYSTAL =
  '<polygon points="330,300 330,396 266,364" fill="#c9bbff"/><polygon points="330,300 394,364 330,396" fill="#a691f6"/><polygon points="266,364 330,396 330,780 266,716" fill="#8a73ea"/><polygon points="330,396 394,364 394,716 330,780" fill="#6a52cf"/><polygon points="330,156 330,198 280,206" fill="#c9bbff"/><polygon points="330,156 380,206 330,198" fill="#a691f6"/><polygon points="280,206 330,198 330,256" fill="#8a73ea"/><polygon points="330,198 380,206 330,256" fill="#6a52cf"/>';

const TILES = {
  light: { tile: '#ffffff', ring: '#32216b' },
  dark: { tile: '#32216b', ring: '#f1eefb' },
} as const;

export type MarkTheme = keyof typeof TILES;

export function markSvg(theme: MarkTheme, size = 1024): string {
  const { tile, ring } = TILES[theme];
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 1024 1024" aria-hidden="true">` +
    `<rect width="1024" height="1024" rx="232" fill="${tile}"/>` +
    CRYSTAL +
    `<circle cx="612" cy="600" r="148" fill="none" stroke="${ring}" stroke-width="96"/></svg>`
  );
}
