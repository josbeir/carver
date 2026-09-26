import { Node, mergeAttributes } from '@tiptap/core';

/**
 * Schema-only Carve frontmatter atom.
 *
 * Carver edits frontmatter through the native GTK document-properties dialog, so the rich
 * editor keeps the block in its projection (and therefore in the canonical Carve it
 * serializes) without rendering any metadata chrome. `CarveKit`'s own frontmatter extension
 * is disabled in favour of this passive node; both share the same node name, attributes, and
 * parse rule so the loader and serializer round-trip it losslessly.
 */
export const InertCarveFrontmatter = Node.create({
  name: 'carveFrontmatter',
  group: 'block',
  atom: true,

  addAttributes() {
    return {
      content: { default: '' },
      format: { default: 'yaml' },
    };
  },

  parseHTML() {
    return [{ tag: 'pre[data-carve-frontmatter]', priority: 60 }];
  },

  renderHTML({ HTMLAttributes, node }) {
    return [
      'pre',
      mergeAttributes(HTMLAttributes, { 'data-carve-frontmatter': 'true' }),
      node.attrs.content,
    ];
  },

  addNodeView() {
    return () => {
      const dom = document.createElement('div');
      dom.className = 'carve-frontmatter-hidden';
      dom.hidden = true;
      dom.contentEditable = 'false';
      return { dom };
    };
  },
});
