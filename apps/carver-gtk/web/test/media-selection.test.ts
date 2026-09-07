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
  marks: { link: { attrs: { href: {}, title: { default: null } } }, bold: {} },
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

  it('keeps adjacent attachment links separate when their marks differ', () => {
    const path = 'assets/brief.pdf';
    const first = schema.mark('link', { href: path, title: 'first' });
    const second = schema.mark('link', { href: path, title: 'second' });
    const doc = schema.node('doc', null, [
      schema.text('First', [first]),
      schema.text('Second', [second]),
    ]);

    expect(mediaOccurrences(doc).map((item) => item.occurrence)).toEqual([
      0, 1,
    ]);
  });
});
