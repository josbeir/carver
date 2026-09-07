import { describe, it, expect } from 'vitest';
import { Schema } from '@tiptap/pm/model';
import { EditorState, NodeSelection, TextSelection } from '@tiptap/pm/state';
import { mediaOccurrences, selectedMedia } from '../src/editor/media-selection';

const schema = new Schema({
  nodes: {
    doc: { content: 'inline*' },
    text: { group: 'inline' },
    image: {
      inline: true,
      group: 'inline',
      atom: true,
      attrs: { src: { default: null } },
    },
  },
  marks: { link: { attrs: { href: {} } }, bold: {} },
});

describe('media selection', () => {
  it('identifies duplicate image occurrences individually', () => {
    const image = schema.node('image', { src: 'assets/a.png' });
    const doc = schema.node('doc', null, [image, schema.text('gap'), image]);
    expect(
      selectedMedia(
        EditorState.create({ doc, selection: NodeSelection.create(doc, 4) }),
      ),
    ).toEqual({ path: 'assets/a.png', occurrence: 1 });
  });

  it('treats a formatted attachment label as one occurrence', () => {
    const link = schema.mark('link', { href: 'assets/brief.pdf' });
    const doc = schema.node('doc', null, [
      schema.text('Project ', [link]),
      schema.text('brief', [link, schema.mark('bold')]),
      schema.text(' gap '),
      schema.text('Again', [link]),
    ]);
    expect(mediaOccurrences(doc)).toHaveLength(2);
    expect(
      selectedMedia(
        EditorState.create({ doc, selection: TextSelection.create(doc, 9) }),
      ),
    ).toEqual({ path: 'assets/brief.pdf', occurrence: 0 });
    expect(
      selectedMedia(
        EditorState.create({ doc, selection: TextSelection.create(doc, 19) }),
      ),
    ).toEqual({ path: 'assets/brief.pdf', occurrence: 1 });
  });

  it('clears media selection in ordinary text and excludes ordinary links', () => {
    const doc = schema.node('doc', null, [
      schema.text('plain'),
      schema.text('web', [
        schema.mark('link', { href: 'https://example.com' }),
      ]),
      schema.node('image'),
    ]);
    expect(mediaOccurrences(doc)).toEqual([]);
    expect(
      selectedMedia(
        EditorState.create({ doc, selection: TextSelection.create(doc, 2) }),
      ),
    ).toBeNull();
  });

  it('keeps adjacent images as separate occurrences', () => {
    const image = schema.node('image', { src: 'assets/a.png' });
    const doc = schema.node('doc', null, [image, image]);
    expect(mediaOccurrences(doc).map((item) => item.occurrence)).toEqual([
      0, 1,
    ]);
  });
});
