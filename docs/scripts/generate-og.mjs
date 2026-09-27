// Generates the social preview card committed as `public/og.png`.
//
// The card is typographic on purpose so it never goes stale the way a
// screenshot would. Run with `npm run og` and commit the result; the build
// itself does not depend on this script.
import { access, readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import sharp from 'sharp';

const iconPath = fileURLToPath(new URL('../public/carver-icon.svg', import.meta.url));
const outputPath = fileURLToPath(new URL('../public/og.png', import.meta.url));

const WIDTH = 1200;
const HEIGHT = 630;

const background = `
<svg width="${WIDTH}" height="${HEIGHT}" viewBox="0 0 ${WIDTH} ${HEIGHT}" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#142820"/>
      <stop offset="1" stop-color="#142820"/>
    </linearGradient>
    <radialGradient id="glow" cx="0.12" cy="-0.05" r="0.9">
      <stop offset="0" stop-color="#8ff0a4" stop-opacity="0.08"/>
      <stop offset="1" stop-color="#8ff0a4" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="${WIDTH}" height="${HEIGHT}" fill="url(#bg)"/>
  <rect width="${WIDTH}" height="${HEIGHT}" fill="url(#glow)"/>
  <text x="96" y="392" font-family="Inter, 'DejaVu Sans', 'Liberation Sans', sans-serif" font-size="92" font-weight="600" fill="#fff8e7">Carver</text>
  <text x="100" y="452" font-family="Inter, 'DejaVu Sans', 'Liberation Sans', sans-serif" font-size="34" fill="#fff8e7" fill-opacity="0.78">A little space for big ideas</text>
  <text x="100" y="524" font-family="Inter, 'DejaVu Sans', 'Liberation Sans', sans-serif" font-size="24" letter-spacing="2" fill="#8ff0a4" fill-opacity="0.9">WRITE · ORGANIZE · MAKE IT YOURS</text>
</svg>`;

await access(iconPath);

const icon = await sharp(await readFile(iconPath))
  .resize(128, 128)
  .png()
  .toBuffer();

await sharp(Buffer.from(background))
  .composite([{ input: icon, top: 108, left: 96 }])
  .png()
  .toFile(outputPath);

// iOS ignores SVG favicons, so also write the PNG it expects.
await sharp(await readFile(iconPath))
  .resize(180, 180)
  .png()
  .toFile(fileURLToPath(new URL('../public/apple-touch-icon.png', import.meta.url)));

console.log(`Wrote ${outputPath}`);
