import assert from 'node:assert/strict';
import test from 'node:test';

import { carveToProseMirrorWithReport, serializeToCarve } from '@markup-carve/carve-grammars/tiptap';
import { unsupportedForEditing } from '../src/editability.js';

test('keeps soft breaks and smart punctuation in Edit mode', () => {
  const source = `# Campagne

*Sociaal secretariaat*

Doegroep:
interimkantoren
hoe kunnen ze helpen

* Direct mailing
* Online campagine (social assets)

## Onepager

- diensten
* directe CTA om contact met "mij" op te nemen
* Of via een afspraken tool (plan.me)

## Algemene voordelen
En eventueel meer in depth via interactieve toggles.`;
  const result = carveToProseMirrorWithReport(source, { unsupported: 'preserve' });

  assert.deepEqual(Object.keys(result.preserved), ['document']);
  assert.deepEqual(Object.keys(result.degraded).sort(), ['smart_punctuation', 'soft_break']);
  assert.deepEqual(unsupportedForEditing(result), []);
  assert.equal(serializeToCarve(result.doc), source);
});
