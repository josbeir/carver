import type { ImageMetadata } from 'astro';

import editorDark from '../assets/screenshots/editor-dark.png';
import editorLight from '../assets/screenshots/editor-light.png';

/** A single captured application view, paired across both UI themes. */
export interface Shot {
  alt: string;
  /** Optional caption; omit when the surrounding copy already describes it. */
  caption?: string;
  light: ImageMetadata;
  dark: ImageMetadata;
}

/**
 * Every application view the site can present.
 *
 * A view only appears once both theme variants exist, so a feature panel never
 * shows an empty slot while captures are still pending. To add one, drop
 * `<name>-light.png` and `<name>-dark.png` into `src/assets/screenshots/`,
 * import them, and fill in its entry below.
 */
export type ShotName = 'editor' | 'source' | 'bases' | 'agent' | 'library' | 'settings';

export const shots: Partial<Record<ShotName, Shot>> = {
  editor: {
    alt: "Editing a note in Carver's rich text editor",
    light: editorLight,
    dark: editorDark,
  },
  // Pending captures, listed here so the shape is obvious when the shots land:
  // source:    { alt: 'Carve source alongside its live preview', ... },
  // bases:     { alt: 'A saved Base with frontmatter columns and filters', ... },
  // agent:     { alt: "Carver's connect-an-agent setup screen", ... },
  // library:   { alt: 'Categories, recent notes, and the recoverable Trash', ... },
  // settings:  { alt: "Carver's preferences", ... },
};
