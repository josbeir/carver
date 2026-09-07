import { describe, expect, it } from 'vitest';

import { focusEmptyEditorSurface } from '../src/editor/empty-surface';

describe('focusEmptyEditorSurface', () => {
  it('focuses an empty editor when its blank writing surface is clicked', () => {
    const root = {};
    const surface = {};
    let prevented = false;
    let focusPosition = null;
    const editor = {
      isEmpty: true,
      view: { dom: surface },
      commands: {
        focus: (position) => {
          focusPosition = position;
        },
      },
    };

    expect(
      focusEmptyEditorSurface(
        {
          target: surface,
          preventDefault: () => {
            prevented = true;
          },
        },
        editor,
        root,
      ),
    ).toBe(true);
    expect(prevented).toBe(true);
    expect(focusPosition).toBe('end');
  });

  it('leaves document content clicks and non-empty editors alone', () => {
    const root = {};
    const surface = {};
    const content = {};
    let focused = false;
    const editor = {
      isEmpty: false,
      view: { dom: surface },
      commands: {
        focus: () => {
          focused = true;
        },
      },
    };
    const event = {
      target: content,
      preventDefault: () => {
        throw new Error('should not prevent');
      },
    };

    expect(focusEmptyEditorSurface(event, editor, root)).toBe(false);
    expect(focused).toBe(false);
  });
});
