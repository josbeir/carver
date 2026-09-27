# Carver docs site

The Carver landing page, built with [Astro](https://astro.build),
[Tailwind CSS](https://tailwindcss.com), and [daisyUI](https://daisyui.com). It
is deployed to GitHub Pages at <https://josbeir.github.io/carver>.

Future documentation pages will live alongside the landing page; the structure
below leaves room for a content-collection docs section without a rewrite.

## Requirements

- Node.js 22 or newer

## Commands

```sh
npm install        # install dependencies
npm run dev        # local dev server at http://localhost:4321/carver
npm run check      # astro check + Prettier (what CI runs)
npm run format     # apply Prettier
npm run build      # static build to dist/
npm run preview    # preview the built site
npm run og         # regenerate public/og.png from the brand palette
```

`npm run check` and `npm run build` are required before opening a pull request;
the `Docs site` workflow runs both.

## Layout

```
docs/
  astro.config.mjs        Astro config (site, base, integrations)
  src/
    assets/screenshots/    application screenshots, one light/dark pair per view
    assets/agent-icons/    agent client logos, normalized to currentColor
    components/            page sections and shared widgets
    data/features.ts       the feature areas shown in the interactive section
    data/screenshots.ts    single source of truth for screenshots
    data/version.ts        Carver version, read from the workspace Cargo.toml
    layouts/BaseLayout.astro
    pages/                 landing page, 404
    styles/global.css      Tailwind import and the two daisyUI themes
    utils/base.ts          base path with a guaranteed trailing slash
  public/                  favicon, app icon, social image
  plans/                   unrelated product notes, ignored by the build
```

The build only reads `src/pages`, so everything else under `docs/` (including
`plans/`) is never published.

## Theming

`src/styles/global.css` defines two daisyUI themes, `carver` (light, default)
and `carver-dark` (follows the OS by default), derived from the application icon
palette. A small inline script in `BaseLayout.astro` applies the stored
preference before first paint, and `ThemeToggle.astro` flips it.

Because the theme drives a `data-theme` attribute, the `dark:` variant is
remapped to it in `global.css`. That is what lets `ThemedScreenshot.astro` show
the dark application screenshot in dark mode using CSS only.

## Screenshots

The screenshots in `src/assets/screenshots/` are generated, not hand-captured:

```sh
./scripts/capture-screenshots.sh
```

The script runs the opt-in `capture_docs_screenshots` GTK test under the same
headless Weston harness as the display-backed test suite. It seeds a neutral
sample library, opens each view, and writes 2x light/dark PNG pairs (the
embedded editor and preview follow the theme as well). Review the images and
commit them.

The scenes live in `apps/carver-gtk/src/tests/ui/screenshots.rs`; add or adjust a
scene there instead of capturing by hand. `src/data/screenshots.ts` is the single
source of truth for which views the page shows, including `alt` text and the
optional `narrow` flag that keeps tall dialog captures from dominating a panel.
A view only renders once both theme variants exist.

## Deployment

The `Docs site` workflow builds `docs/` and publishes `docs/dist` through GitHub
Pages. Publishing through Actions (rather than a branch folder) keeps the Astro
source and `plans/` out of the deployed site.

One-time setup: in the repository settings, set **Pages → Build and deployment →
Source** to **GitHub Actions**.

### Custom domain later

The site is a project page, so `astro.config.mjs` sets `base: '/carver'` and all
internal links go through `src/utils/base.ts`. To move to a custom domain, set
`site` to the new origin, remove `base`, and add the `CNAME` file that GitHub
Pages requires.
