import { describe, expect, it } from 'vitest';

import { getSchema } from '@tiptap/core';
import {
  CarveKit,
  carveToProseMirrorWithReport,
} from '@markup-carve/carve-grammars/tiptap';
import {
  AllSelection,
  EditorState,
  NodeSelection,
  TextSelection,
} from '@tiptap/pm/state';

import { selectedCarveSource } from '../src/editor/selection-copy';

const schema = getSchema([CarveKit]);

function documentFrom(source: string) {
  return schema.nodeFromJSON(
    carveToProseMirrorWithReport(source, { unsupported: 'preserve' }).doc,
  );
}

function textRange(document, text: string): [number, number] {
  let range: [number, number] | null = null;
  document.descendants((node, position) => {
    const offset = node.isText ? node.text?.indexOf(text) : undefined;
    if (offset != null && offset >= 0) {
      range = [position + offset, position + offset + text.length];
      return false;
    }
    return range == null;
  });
  if (!range) throw new Error(`Text not found: ${text}`);
  return range;
}

describe('selection copy', () => {
  it('does not replace native behavior for an empty selection', () => {
    const document = documentFrom('Text');
    expect(
      selectedCarveSource(EditorState.create({ doc: document })),
    ).toBeNull();
  });

  it('keeps inline formatting without adding an enclosing list marker', () => {
    const document = documentFrom('- Before *bold* after');
    const [from, to] = textRange(document, 'bold');
    const state = EditorState.create({
      doc: document,
      selection: TextSelection.create(document, from, to),
    });

    expect(selectedCarveSource(state)).toBe('*bold*');
  });

  it('preserves block structure when the whole document is selected', () => {
    const source = '# Heading\n\n- First\n- Second';
    const document = documentFrom(source);
    const state = EditorState.create({
      doc: document,
      selection: new AllSelection(document),
    });

    expect(selectedCarveSource(state)).toBe(source);
  });

  it('keeps a selected managed image available to the native clipboard renderer', () => {
    const document = documentFrom('Before ![Alt](assets/image.png) after');
    let imagePosition = 0;
    document.descendants((node, position) => {
      if (node.type.name === 'image') imagePosition = position;
    });
    const state = EditorState.create({
      doc: document,
      selection: NodeSelection.create(document, imagePosition),
    });

    expect(selectedCarveSource(state)).toBe('![Alt](assets/image.png)');
  });
});
