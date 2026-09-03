import assert from 'node:assert/strict';
import test from 'node:test';

import {
  carveToProseMirrorWithReport,
  serializeToCarve,
} from '@markup-carve/carve-grammars/tiptap';

test('preserves editable Carve blocks through the ProseMirror bridge', () => {
  const source = '# Heading\n\nParagraph with *bold* and /italic/.\n\n```rust\nlet value = 1;\n```';
  const result = carveToProseMirrorWithReport(source, { unsupported: 'preserve' });

  assert.deepEqual(result.preserved, {});
  assert.deepEqual(result.degraded, {});
  assert.equal(serializeToCarve(result.doc), source);
});

test('keeps tables editable rather than flattening them into text', () => {
  const source = '|= Name |= State |\n| Carver | Ready |';
  const result = carveToProseMirrorWithReport(source, { unsupported: 'preserve' });

  assert.equal(result.doc.content[0].type, 'table');
  assert.equal(serializeToCarve(result.doc), source);
});

test('keeps task lists editable and preserves their checked state', () => {
  const source = '- [x] Complete\n- [ ] Open';
  const result = carveToProseMirrorWithReport(source, { unsupported: 'preserve' });

  assert.equal(result.doc.content[0].type, 'taskList');
  assert.equal(result.doc.content[0].content[0].attrs.checked, true);
  assert.equal(result.doc.content[0].content[1].attrs.checked, false);
  assert.equal(serializeToCarve(result.doc), source);
});

test('keeps adjacent images editable through a lossless source envelope', () => {
  const source = '![First](assets/first.png){width="50%"}![Second](assets/second.png)';
  const result = carveToProseMirrorWithReport(source, { unsupported: 'preserve' });

  assert.equal(result.doc.content[0].type, 'paragraph');
  assert.notEqual(result.doc.content[0].type, 'carveUnsupported');
  assert.equal(serializeToCarve(result.doc), source);
});

test('serializes image blocks with a blank line between them', () => {
  const document = {
    type: 'doc',
    content: [
      { type: 'paragraph', content: [{ type: 'image', attrs: { alt: 'First', src: 'assets/first.png', carveKeyValues: { width: '50%' } } }] },
      { type: 'paragraph', content: [{ type: 'image', attrs: { alt: 'Second', src: 'assets/second.png' } }] },
    ],
  };

  assert.equal(
    serializeToCarve(document),
    '![First](assets/first.png){width="50%"}\n\n![Second](assets/second.png)',
  );
});
