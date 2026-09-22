import { describe, expect, it } from 'vitest';
import { getSchema } from '@tiptap/core';
import { CarveKit } from '@markup-carve/carve-grammars/tiptap';

import { mightBeMarkupText, plainTextSlice } from '../src/editor/paste-format';

describe('mightBeMarkupText', () => {
  it('leaves ordinary single-line prose to the native paste path', () => {
    expect(mightBeMarkupText('An ordinary sentence.')).toBe(false);
    expect(mightBeMarkupText('https://example.com/path')).toBe(false);
  });

  it('recognizes common block and inline markup', () => {
    for (const text of [
      '**bold**',
      '# Heading',
      '- item one\n- item two',
      '> quote',
      'a paragraph\n\nanother paragraph',
      'Text with a [link](https://example.com)',
      'A paragraph{role="note"}',
      '`code`',
    ]) {
      expect(mightBeMarkupText(text), text).toBe(true);
    }
  });
});

describe('plainTextSlice', () => {
  const schema = getSchema([CarveKit]);

  it('splits blank-line-separated blocks into paragraphs', () => {
    const slice = plainTextSlice(schema, 'First\n\nSecond');

    expect(slice.content.childCount).toBe(2);
    expect(slice.content.child(0).textContent).toBe('First');
    expect(slice.content.child(1).textContent).toBe('Second');
  });

  it('keeps in-paragraph newlines as hard breaks', () => {
    const slice = plainTextSlice(schema, 'First\nSecond');
    const paragraph = slice.content.child(0);

    expect(paragraph.childCount).toBe(3);
    expect(paragraph.child(1).type.name).toBe('hardBreak');
  });
});
