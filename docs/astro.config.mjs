// @ts-check
import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';
import tailwindcss from '@tailwindcss/vite';

// The site is published as a GitHub Pages project site at
// https://josbeir.github.io/carver. Moving to a custom domain later is a
// two-line change: drop `base` and set `site` to the new origin.
export default defineConfig({
  site: 'https://josbeir.github.io',
  base: '/carver',
  // One canonical URL form: GitHub Pages serves `/carver/`, so keep the trailing
  // slash everywhere and the sitemap will only carry that URL.
  trailingSlash: 'always',
  integrations: [sitemap()],
  vite: {
    plugins: [tailwindcss()],
  },
});
