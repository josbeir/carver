import assert from 'node:assert/strict';
import test from 'node:test';

import { focusEmptyEditorSurface } from '../src/empty-surface.js';

test('focuses an empty editor when its blank writing surface is clicked', () => {
  const root = {};
  const surface = {};
  let prevented = false;
  let focusPosition = null;
  const editor = {
    isEmpty: true,
    view: { dom: surface },
    commands: { focus: position => { focusPosition = position; } },
  };

  assert.equal(focusEmptyEditorSurface({
    target: surface,
    preventDefault: () => { prevented = true; },
  }, editor, root), true);
  assert.equal(prevented, true);
  assert.equal(focusPosition, 'end');
});

test('leaves document content clicks and non-empty editors alone', () => {
  const root = {};
  const surface = {};
  const content = {};
  let focused = false;
  const editor = {
    isEmpty: false,
    view: { dom: surface },
    commands: { focus: () => { focused = true; } },
  };
  const event = { target: content, preventDefault: () => assert.fail('should not prevent') };

  assert.equal(focusEmptyEditorSurface(event, editor, root), false);
  assert.equal(focused, false);
});
