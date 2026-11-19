/** Two home layouts to compare: `?layout=a` (banner, default) or `?layout=b` (card). */
export const layout: 'a' | 'b' =
  typeof location !== 'undefined' && new URLSearchParams(location.search).get('layout') === 'b' ? 'b' : 'a';
