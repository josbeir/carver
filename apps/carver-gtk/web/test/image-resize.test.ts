import { describe, expect, it } from 'vitest';

import { getSchema } from '@tiptap/core';
import Image from '@tiptap/extension-image';
import StarterKit from '@tiptap/starter-kit';
import { NodeSelection, EditorState } from '@tiptap/pm/state';

import { resizeSelectedImage } from '../src/editor/image-resize';

const CarveImage = Image.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      carveKeyValues: { default: null },
    };
  },
});

function imageState(width) {
  const schema = getSchema([StarterKit, CarveImage]);
  const image = schema.nodes.image.create({
    src: 'assets/diagram.png',
    alt: 'Diagram',
    carveKeyValues: { width: `${width}%` },
  });
  const document = schema.topNodeType.create(null, [image]);
  return EditorState.create({
    schema,
    doc: document,
    selection: NodeSelection.create(document, 0),
  });
}

describe('resizeSelectedImage', () => {
  it('keeps an image selected after repeated width changes', () => {
    const initial = imageState(25);
    const first = resizeSelectedImage(initial, 50);
    expect(first).not.toBeNull();
    const resized = initial.apply(first);
    expect(resized.selection).toBeInstanceOf(NodeSelection);

    const second = resizeSelectedImage(resized, 75);
    expect(second).not.toBeNull();
    const twiceResized = resized.apply(second);
    expect(twiceResized.selection).toBeInstanceOf(NodeSelection);
    expect(twiceResized.doc.firstChild?.attrs.carveKeyValues.width).toBe('75%');
  });

  it('clears an image width and ignores non-image selections', () => {
    const initial = imageState(25);
    const cleared = resizeSelectedImage(initial, 0);
    expect(cleared).not.toBeNull();
    expect(initial.apply(cleared).doc.firstChild?.attrs.carveKeyValues).toEqual(
      {},
    );

    const schema = getSchema([StarterKit, CarveImage]);
    const document = schema.topNodeType.createAndFill();
    const state = EditorState.create({ schema, doc: document });
    expect(resizeSelectedImage(state, 50)).toBeNull();
  });

  it('adds a width when imported image metadata is absent', () => {
    const schema = getSchema([StarterKit, CarveImage]);
    const image = schema.nodes.image.create({
      src: 'assets/diagram.png',
      carveKeyValues: null,
    });
    const document = schema.topNodeType.create(null, [image]);
    const state = EditorState.create({
      schema,
      doc: document,
      selection: NodeSelection.create(document, 0),
    });
    const transaction = resizeSelectedImage(state, 50);
    expect(
      state.apply(transaction).doc.firstChild?.attrs.carveKeyValues,
    ).toEqual({ width: '50%' });
  });
});
