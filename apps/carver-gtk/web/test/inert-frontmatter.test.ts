// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';
import { Editor, getSchema } from '@tiptap/core';
import {
  CarveKit,
  carveToProseMirrorWithReport,
  serializeToCarve,
} from '@markup-carve/carve-grammars/tiptap';

import { InertCarveFrontmatter } from '../src/editor/inert-frontmatter';

describe('InertCarveFrontmatter', () => {
  it('registers the carveFrontmatter node without CarveKit owning it', () => {
    const schema = getSchema([
      CarveKit.configure({ image: false, carveFrontmatter: false }),
      InertCarveFrontmatter,
    ]);

    expect(schema.nodes.carveFrontmatter).toBeDefined();
  });

  it('keeps frontmatter in the projection without rendering a card', () => {
    const source =
      '---\npriority: 4\ntags: [rust, notes]\n---\n\n# Metadata-aware note';
    const result = carveToProseMirrorWithReport(source, {
      unsupported: 'preserve',
    });
    const element = document.createElement('div');
    const editor = new Editor({
      element,
      extensions: [
        CarveKit.configure({ image: false, carveFrontmatter: false }),
        InertCarveFrontmatter,
      ],
    });

    editor.commands.setContent(result.doc);

    expect(editor.getJSON().content?.[0]).toMatchObject({
      type: 'carveFrontmatter',
      attrs: {
        format: 'yaml',
        content: 'priority: 4\ntags: [rust, notes]',
      },
    });
    expect(serializeToCarve(editor.getJSON())).toBe(source);
    expect(element.querySelector('.carve-frontmatter-card')).toBeNull();
    expect(element.querySelector('.carve-frontmatter-hidden')).not.toBeNull();

    editor.destroy();
  });
});
