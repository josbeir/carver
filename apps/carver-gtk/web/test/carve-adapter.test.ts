import { describe, expect, it } from 'vitest';

import {
  carveToProseMirrorWithReport,
  serializeToCarve,
} from '@markup-carve/carve-grammars/tiptap';

describe('Carve adapter', () => {
  it('preserves editable Carve blocks through the ProseMirror bridge', () => {
    const source =
      '# Heading\n\nParagraph with *bold* and /italic/.\n\n```rust\nlet value = 1;\n```';
    const result = carveToProseMirrorWithReport(source, {
      unsupported: 'preserve',
    });

    expect(result.preserved).toEqual({});
    expect(result.degraded).toEqual({});
    expect(serializeToCarve(result.doc)).toBe(source);
  });

  it('keeps tables editable rather than flattening them into text', () => {
    const source = '|= Name |= State |\n| Carver | Ready |';
    const result = carveToProseMirrorWithReport(source, {
      unsupported: 'preserve',
    });

    expect(result.doc.content?.[0]?.type).toBe('table');
    expect(serializeToCarve(result.doc)).toBe(source);
  });

  it('keeps task lists editable and preserves their checked state', () => {
    const source = '- [x] Complete\n- [ ] Open';
    const result = carveToProseMirrorWithReport(source, {
      unsupported: 'preserve',
    });

    expect(result.doc.content?.[0]?.type).toBe('taskList');
    expect(result.doc.content?.[0]?.content?.[0]?.attrs?.checked).toBe(true);
    expect(result.doc.content?.[0]?.content?.[1]?.attrs?.checked).toBe(false);
    expect(serializeToCarve(result.doc)).toBe(source);
  });

  it('keeps adjacent images editable through a lossless source envelope', () => {
    const source =
      '![First](assets/first.png){width="50%"}![Second](assets/second.png)';
    const result = carveToProseMirrorWithReport(source, {
      unsupported: 'preserve',
    });

    expect(result.doc.content?.[0]?.type).toBe('paragraph');
    expect(result.doc.content?.[0]?.type).not.toBe('carveUnsupported');
    expect(serializeToCarve(result.doc)).toBe(source);
  });

  it('keeps image soft breaks as inline whitespace for the editor', () => {
    const source = '![First](assets/first.png)\n![Second](assets/second.png)';
    const result = carveToProseMirrorWithReport(source, {
      unsupported: 'preserve',
    });
    const content = result.doc.content?.[0]?.content;

    expect(result.doc.content).toHaveLength(1);
    expect(content?.[0]?.type).toBe('image');
    expect(content?.[1]).toEqual({ type: 'text', text: '\n' });
    expect(content?.[2]?.type).toBe('image');
    expect(serializeToCarve(result.doc)).toBe(source);
  });

  it('serializes image blocks with a blank line between them', () => {
    const document = {
      type: 'doc',
      content: [
        {
          type: 'paragraph',
          content: [
            {
              type: 'image',
              attrs: {
                alt: 'First',
                src: 'assets/first.png',
                carveKeyValues: { width: '50%' },
              },
            },
          ],
        },
        {
          type: 'paragraph',
          content: [
            {
              type: 'image',
              attrs: { alt: 'Second', src: 'assets/second.png' },
            },
          ],
        },
      ],
    };

    expect(serializeToCarve(document)).toBe(
      '![First](assets/first.png){width="50%"}\n\n![Second](assets/second.png)',
    );
  });
});
