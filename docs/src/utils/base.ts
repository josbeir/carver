/**
 * The site base path with a guaranteed trailing slash, so relative asset paths
 * can be concatenated safely (`${base}og.png`).
 *
 * Astro's `import.meta.env.BASE_URL` omits the trailing slash when
 * `trailingSlash: 'ignore'` is set, which would otherwise produce paths like
 * `/carverfavicon.svg`.
 */
export const base = import.meta.env.BASE_URL.replace(/\/?$/, '/');
