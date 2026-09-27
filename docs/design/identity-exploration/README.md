# Carver identity exploration

**Selected: Folded C.** This board is retained as the original design comparison.
The app and website now use this direction; the other concepts remain references.

Open `index.html` directly in a browser, or serve this directory locally:

```sh
python3 -m http.server 8765 --bind 127.0.0.1 --directory docs/design/identity-exploration
```

Visit http://127.0.0.1:8765. Select a concept to compare its actual-size icons,
wordmark, desktop neighbours, light/dark website mockups, and social card.
All fonts and graphics are local; the board works offline. External links are
references only. Website CTA labels are presentation mockups, not active controls.

## Recommendation

**Folded C** has the clearest signature: a geometric paper strip with an open
silhouette and two cream folds. **Cut paper** is more immediately recognizable
as a note, but feels heavier. **Curled sheet** is softer and tactile, but can read
as a ribbon or crescent. Folded C was selected for production; the remaining candidates are retained for comparison.

Nine editable SVGs are in `concepts/`: a colour icon, symbolic, and favicon for
each direction. Named groups separate materials in the full-colour SVGs.
Symbolics use a 16px grid and a single fill; the board inverts them for dark
backgrounds. Production uses the Folded C symbolic and a theme-aware website favicon.

The wordmark is live Inter Semibold text paired with the icon, not a new font or
outlined logo. The mockups retain the existing hero copy and use Inter headings.
The social-card mockups use a 1200:630 aspect ratio on desktop and reflow on narrow screens. The board itself is a historical comparison, separate from production assets.

## Construction and review

- Full-colour artboards are 128×128 with geometry on a 2px grid. The lower profile
  ends at y=116, within the official template's baseline band centred at y=117.
- Main silhouettes span roughly 84–88px horizontally and 96–100px vertically,
  compared against equal-size GNOME reference icons on light and dark surfaces.
- Planar surfaces use flat colour. Only the curled underside uses a gradient.
  No external shadows, embedded raster images, fonts, scripts, or external
  references are needed to render the candidate SVGs.
- At 32px, the silhouettes remain distinct; the cream fold detail becomes secondary.
  At 16px, the purpose-drawn symbolic/favicon removes material detail.
- Green #2EC27E and #26A269 follow GNOME Green-4 and Green-5. Cream, ink, and
  deeper greens are supporting brand colours, listed on the board.

Calculated WCAG contrast ratios for the website mockups:

| Pair                                |   Ratio |
| ----------------------------------- | ------: |
| Light text #17352B / #FFFDF5        | 13.05:1 |
| Light muted #52675B / #FFFDF5       |  5.98:1 |
| Light button text #FFF8E7 / #176443 |  6.75:1 |
| Dark text #FFF8E7 / #142820         | 14.65:1 |
| Dark muted #C7D7CD / #142820        | 10.36:1 |
| Dark button text #142820 / #8FF0A4  | 11.20:1 |

All listed text pairs exceed 4.5:1. Colours within the decorative icon are not
text contrast pairs. This is a visual concept review, not a claim of formal
GNOME design approval or a complete accessibility audit.

## Validation results

- Browser review at 1280px, the default panel width, and 390px: no horizontal
  overflow after fixing the narrow social-card layout.
- All three concept selectors activate their matching study; all images and the
  bundled Inter font load successfully.
- Colour, symbolic, and favicon SVGs parse successfully without linked dependencies.
- Light/dark desktop comparisons, size studies, wordmarks, and website treatments
  reviewed visually. Contrast ratios above calculated from sRGB relative luminance.
- `npm run check` and `npm run build` pass from `docs/`. This directory is absent
  from the production output.

## Sources and licenses

- [GNOME app-icon guidelines](https://developer.gnome.org/hig/guidelines/app-icons.html)
  and [official template](https://gitlab.gnome.org/Teams/Design/HIG-app-icons/-/blob/master/template.svg)
  informed proportions, grid, materials, and baseline. The template is not bundled.
- `reference/files.svg`: unchanged installed `org.gnome.Nautilus.svg`, credited
  in its embedded metadata to the GNOME Design Team under CC BY-SA 4.0.
  [Upstream source](https://gitlab.gnome.org/GNOME/nautilus).
  License text: `reference/CC-BY-SA-4.0.txt`.
- `reference/calendar.svg` and `reference/calculator.svg`: unchanged installed
  GNOME Calendar and Calculator app icons. Their packages declare GPL-3.0-or-later;
  included here as comparison references, not Carver assets.
  [Calendar source](https://gitlab.gnome.org/GNOME/gnome-calendar),
  [Calculator source](https://gitlab.gnome.org/GNOME/gnome-calculator).
  License text: `reference/GPL-3.0.txt`.
- `assets/Inter.woff2`: Inter Variable by Rasmus Andersson, copied from the local
  Blender font distribution. SIL Open Font License 1.1; see `assets/Inter-LICENSE.txt`
  and [Inter upstream](https://github.com/rsms/inter).
- New Carver concept SVGs and board code follow the repository's MIT license.
  Third-party reference artwork retains its own license.

This directory sits outside `docs/src/pages` and `docs/public`, so Astro does not
publish it. The selected direction is now applied to the app and website.
