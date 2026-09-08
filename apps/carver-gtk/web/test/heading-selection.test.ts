import { describe, it, expect } from 'vitest';
import { Schema } from '@tiptap/pm/model';
import { EditorState, TextSelection } from '@tiptap/pm/state';
import {
  headingOccurrences,
  selectedHeading,
} from '../src/editor/heading-selection';

const schema = new Schema({
  nodes: {
    doc: { content: 'block*' },
    text: { group: 'inline' },
    heading: { group: 'block', content: 'inline*' },
    paragraph: { group: 'block', content: 'inline*' },
    blockquote: { group: 'block', content: 'block*' },
  },
});
const heading = () => schema.node('heading', null, schema.text('Same 🦀'));

describe('heading navigation', () => {
  it('maps duplicate and nested headings in authored order', () => {
    const doc = schema.node('doc', null, [
      heading(),
      schema.node('blockquote', null, heading()),
    ]);
    const occurrences = headingOccurrences(doc);
    expect(occurrences).toHaveLength(2);
    const selection = TextSelection.create(doc, occurrences[1].from);
    expect(selectedHeading(EditorState.create({ doc, selection }))).toBe(1);
  });

  it('clears the heading when the caret enters its following paragraph', () => {
    const doc = schema.node('doc', null, [
      heading(),
      schema.node('paragraph', null, schema.text('body')),
    ]);
    const selection = TextSelection.create(doc, doc.child(0).nodeSize + 1);
    expect(selectedHeading(EditorState.create({ doc, selection }))).toBeNull();
  });

  it('does not select a heading for a range spanning multiple blocks', () => {
    const doc = schema.node('doc', null, [heading(), heading()]);
    const positions = headingOccurrences(doc);
    const selection = TextSelection.create(
      doc,
      positions[0].from,
      positions[1].to,
    );
    expect(selectedHeading(EditorState.create({ doc, selection }))).toBeNull();
  });

  it('can select an empty heading', () => {
    const doc = schema.node('doc', null, schema.node('heading'));
    expect(selectedHeading(EditorState.create({ doc }))).toBe(0);
  });
});
