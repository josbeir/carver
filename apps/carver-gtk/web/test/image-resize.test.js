import assert from 'node:assert/strict';
import test from 'node:test';

import { getSchema } from '@tiptap/core';
import Image from '@tiptap/extension-image';
import StarterKit from '@tiptap/starter-kit';
import { NodeSelection, EditorState } from '@tiptap/pm/state';

import { resizeSelectedImage } from '../src/image-resize.js';

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

test('keeps an image selected after repeated width changes', () => {
  const initial = imageState(25);
  const first = resizeSelectedImage(initial, 50);
  assert.ok(first);
  const resized = initial.apply(first);
  assert.ok(resized.selection instanceof NodeSelection);

  const second = resizeSelectedImage(resized, 75);
  assert.ok(second);
  const twiceResized = resized.apply(second);
  assert.ok(twiceResized.selection instanceof NodeSelection);
  assert.equal(twiceResized.doc.firstChild.attrs.carveKeyValues.width, '75%');
});
