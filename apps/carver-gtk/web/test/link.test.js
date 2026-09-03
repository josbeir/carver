import assert from 'node:assert/strict';
import test from 'node:test';

import { getSchema } from '@tiptap/core';
import Link from '@tiptap/extension-link';
import StarterKit from '@tiptap/starter-kit';
import { EditorState, TextSelection } from '@tiptap/pm/state';

import { insertOrUpdateLink, linkContext } from '../src/link.js';

function linkedState(selection = 3) {
  const schema = getSchema([StarterKit, Link]);
  const link = schema.marks.link.create({ href: 'https://old.example' });
  const paragraph = schema.nodes.paragraph.create(null, [schema.text('Carve', [link])]);
  const document = schema.topNodeType.create(null, [paragraph]);
  return EditorState.create({
    schema,
    doc: document,
    selection: TextSelection.create(document, selection),
  });
}

test('link context expands a cursor inside a link to the complete link', () => {
  const context = linkContext(linkedState());
  assert.deepEqual(context, { text: 'Carve', destination: 'https://old.example' });
});

test('link update replaces the complete active link', () => {
  const state = linkedState();
  const transaction = insertOrUpdateLink(state, {
    text: 'Markup Carve',
    destination: 'https://markup-carve.dev',
  });
  const updated = state.apply(transaction);
  assert.equal(updated.doc.textContent, 'Markup Carve');
  assert.equal(updated.doc.firstChild.firstChild.marks[0].attrs.href, 'https://markup-carve.dev');
});
