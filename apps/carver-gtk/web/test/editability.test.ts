import { describe, expect, it } from 'vitest';

import {
  carveToProseMirrorWithReport,
  serializeToCarve,
} from '@markup-carve/carve-grammars/tiptap';
import {
  unsupportedForEditing,
  unsupportedForPasting,
} from '../src/editor/editability';

describe('unsupportedForEditing', () => {
  it('returns preserved keys when the entire document is opaque', () => {
    expect(
      unsupportedForEditing({
        doc: { content: [{ type: 'carveUnsupported' }] },
        preserved: { document: 'unsupported', metadata: 'also unsupported' },
      }),
    ).toEqual(['document', 'metadata']);
  });

  it('treats an opaque document without preservation metadata as unsupported', () => {
    expect(
      unsupportedForEditing({
        doc: { content: [{ type: 'carveUnsupported' }] },
      }),
    ).toEqual([]);
  });

  it('does not reject partially opaque or incomplete adapter results', () => {
    expect(unsupportedForEditing({ doc: { content: [] } })).toEqual([]);
    expect(
      unsupportedForEditing({
        doc: { content: [{ type: 'paragraph' }, { type: 'carveUnsupported' }] },
        preserved: { document: 'unsupported' },
      }),
    ).toEqual([]);
  });

  it('keeps soft breaks and smart punctuation in Edit mode', () => {
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
    const result = carveToProseMirrorWithReport(source, {
      unsupported: 'preserve',
    });

    expect(Object.keys(result.preserved)).toEqual(['document']);
    expect(Object.keys(result.degraded).sort()).toEqual([
      'smart_punctuation',
      'soft_break',
    ]);
    expect(unsupportedForEditing(result)).toEqual([]);
    expect(serializeToCarve(result.doc)).toBe(source);
  });
});

describe('unsupportedForPasting', () => {
  it('rejects source that depends on document-level preservation', () => {
    const result = carveToProseMirrorWithReport(
      '![First](assets/first.png){width="50%"}![Second](assets/second.png)',
      { unsupported: 'preserve' },
    );

    expect(result.preserved).toHaveProperty('document');
    expect(unsupportedForPasting(result)).toEqual(['document']);
  });

  it('accepts source represented entirely by slice content', () => {
    const result = carveToProseMirrorWithReport('Selected', {
      unsupported: 'preserve',
    });

    expect(unsupportedForPasting(result)).toEqual([]);
  });
});
