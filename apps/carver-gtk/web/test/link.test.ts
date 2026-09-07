import { describe, expect, it } from 'vitest';

import { getSchema } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { EditorState, TextSelection } from '@tiptap/pm/state';
import { Schema } from '@tiptap/pm/model';

import { insertOrUpdateLink, linkContext } from '../src/editor/link';

function linkedState(selection = 3) {
  const schema = getSchema([StarterKit]);
  const link = schema.marks.link.create({ href: 'https://old.example' });
  const paragraph = schema.nodes.paragraph.create(null, [
    schema.text('Carve', [link]),
  ]);
  const document = schema.topNodeType.create(null, [paragraph]);
  return EditorState.create({
    schema,
    doc: document,
    selection: TextSelection.create(document, selection),
  });
}

describe('link commands', () => {
  it('expands a cursor inside a link to the complete link', () => {
    const context = linkContext(linkedState());
    expect(context).toEqual({
      text: 'Carve',
      destination: 'https://old.example',
    });
  });

  it('replaces the complete active link', () => {
    const state = linkedState();
    const transaction = insertOrUpdateLink(state, {
      text: 'Markup Carve',
      destination: 'https://markup-carve.dev',
    });
    const updated = state.apply(transaction);
    expect(updated.doc.textContent).toBe('Markup Carve');
    expect(updated.doc.firstChild?.firstChild?.marks[0]?.attrs.href).toBe(
      'https://markup-carve.dev',
    );
  });

  it('inserts a link over the current text selection', () => {
    const schema = getSchema([StarterKit]);
    const document = schema.topNodeType.create(null, [
      schema.nodes.paragraph.create(null, [schema.text('Carve')]),
    ]);
    const state = EditorState.create({
      schema,
      doc: document,
      selection: TextSelection.create(document, 1, 6),
    });
    const transaction = insertOrUpdateLink(state, {
      text: 'Carve docs',
      destination: ' https://markup-carve.dev ',
    });
    expect(transaction).toBeDefined();
    const updated = state.apply(transaction);
    expect(updated.doc.textContent).toBe('Carve docs');
    expect(updated.doc.firstChild?.firstChild?.marks[0]?.attrs.href).toBe(
      'https://markup-carve.dev',
    );
  });

  it('rejects empty link content and reports selected non-link text', () => {
    const schema = getSchema([StarterKit]);
    const document = schema.topNodeType.create(null, [
      schema.nodes.paragraph.create(null, [schema.text('Carve')]),
    ]);
    const state = EditorState.create({
      schema,
      doc: document,
      selection: TextSelection.create(document, 1, 6),
    });
    expect(linkContext(state)).toEqual({ text: 'Carve', destination: '' });
    expect(
      insertOrUpdateLink(state, {
        text: '',
        destination: 'https://markup-carve.dev',
      }),
    ).toBeUndefined();
    expect(
      insertOrUpdateLink(state, { text: 'Carve', destination: '   ' }),
    ).toBeUndefined();
  });

  it('returns no link command when a schema does not support links', () => {
    const schema = new Schema({
      nodes: {
        doc: { content: 'paragraph+' },
        paragraph: { content: 'text*' },
        text: {},
      },
      marks: {},
    });
    const document = schema.topNodeType.create(null, [
      schema.nodes.paragraph.create(null, [schema.text('Carve')]),
    ]);
    const state = EditorState.create({ schema, doc: document });
    expect(linkContext(state)).toEqual({ text: '', destination: '' });
    expect(
      insertOrUpdateLink(state, {
        text: 'Carve',
        destination: 'https://markup-carve.dev',
      }),
    ).toBeUndefined();
  });
});
