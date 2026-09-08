import { Fragment, Schema, Slice } from '@tiptap/pm/model';
import { getSchema } from '@tiptap/core';
import { describe, expect, it } from 'vitest';

import {
  CarveKit,
  serializeToCarve,
} from '@markup-carve/carve-grammars/tiptap';
import {
  ClipboardPasteSanitizer,
  sanitizePastedSlice,
} from '../src/editor/paste-sanitizer';

const schema = new Schema({
  nodes: {
    doc: { content: 'block+' },
    heading: {
      attrs: {
        level: { default: 1 },
        id: { default: null },
        class: { default: null },
        carveKeyValues: { default: null },
        carveAttrOrder: { default: null },
      },
      content: 'inline*',
      group: 'block',
    },
    paragraph: {
      attrs: {
        id: { default: null },
        class: { default: null },
        carveKeyValues: { default: null },
        carveAttrOrder: { default: null },
      },
      content: 'inline*',
      group: 'block',
    },
    text: { group: 'inline' },
    mention: {
      attrs: { id: {} },
      group: 'inline',
      inline: true,
    },
  },
  marks: {
    bold: {},
    link: {
      attrs: {
        href: {},
        id: { default: null },
        class: { default: null },
        carveKeyValues: { default: null },
        carveAttrOrder: { default: null },
      },
    },
  },
});

describe('sanitizePastedSlice', () => {
  it('removes foreign presentation attributes from pasted block nodes', () => {
    const heading = schema.nodes.heading.create(
      {
        level: 2,
        id: 'meeting-notes',
        class: 'ProsemirrorEditor-heading',
        carveKeyValues: {
          style: 'color: rgb(255, 255, 255)',
          'data-pm-slice': '0 0 []',
        },
        carveAttrOrder: null,
      },
      schema.text('Meeting notes'),
    );

    const sanitized = sanitizePastedSlice(
      new Slice(Fragment.from(heading), 0, 0),
    );

    expect(sanitized.content.firstChild?.attrs).toEqual({
      level: 2,
      id: null,
      class: null,
      carveKeyValues: null,
      carveAttrOrder: null,
    });
  });

  it('preserves supported formatting and semantic mark attributes', () => {
    const link = schema.marks.link.create({
      href: 'https://example.com',
      id: 'foreign-id',
      class: 'foreign-link',
      carveKeyValues: { style: 'color: blue' },
      carveAttrOrder: null,
    });
    const bold = schema.marks.bold.create();
    const heading = schema.nodes.heading.create(
      { level: 3 },
      schema.text('Linked', [link, bold]),
    );

    const sanitized = sanitizePastedSlice(
      new Slice(Fragment.from(heading), 0, 0),
    );

    expect(sanitized.content.firstChild?.firstChild?.marks).toEqual([
      bold,
      schema.marks.link.create({ href: 'https://example.com' }),
    ]);
  });

  it('preserves the clipboard slice boundaries', () => {
    const heading = schema.nodes.heading.create(
      { level: 2 },
      schema.text('Partial heading'),
    );

    const sanitized = sanitizePastedSlice(
      new Slice(Fragment.from(heading), 1, 1),
    );

    expect([sanitized.openStart, sanitized.openEnd]).toEqual([1, 1]);
  });

  it('preserves semantic attributes that are not Carve attribute slots', () => {
    const heading = schema.nodes.heading.create(
      { level: 2 },
      schema.nodes.mention.create({ id: 'project' }),
    );

    const sanitized = sanitizePastedSlice(
      new Slice(Fragment.from(heading), 0, 0),
    );

    expect(sanitized.content.firstChild?.firstChild?.attrs).toEqual({
      id: 'project',
    });
  });

  it('carries canonical source from the native clipboard format', () => {
    const source = '## R\u00e9sum\u00e9{#references role="bibliography"}';
    const sanitizer = new ClipboardPasteSanitizer();

    sanitizer.capturePastedSource(source);
    expect(sanitizer.takePastedSource()).toBe(source);
    expect(sanitizer.takePastedSource()).toBeNull();
  });

  it('clears a stale native clipboard source when the next paste is external', () => {
    const sanitizer = new ClipboardPasteSanitizer();

    sanitizer.capturePastedSource('Internal');
    sanitizer.capturePastedSource(null);
    expect(sanitizer.takePastedSource()).toBeNull();
  });

  it('refuses an unsupported Carver source instead of degrading its HTML', () => {
    const heading = schema.nodes.heading.create(
      { level: 2 },
      schema.text('Rendered fallback'),
    );
    const parsed = new Slice(Fragment.from(heading), 0, 0);
    const sanitizer = new ClipboardPasteSanitizer();
    sanitizer.capturePastedSource('Unsupported canonical source');

    expect(sanitizer.resolvePastedSlice(parsed, false, () => null)).toEqual(
      Slice.empty,
    );
  });

  it('keeps plain-text paste semantics when the custom format is present', () => {
    const heading = schema.nodes.heading.create(
      { level: 2 },
      schema.text('Plain text'),
    );
    const parsed = new Slice(Fragment.from(heading), 0, 0);
    const sanitizer = new ClipboardPasteSanitizer();
    sanitizer.capturePastedSource('Canonical source');

    expect(
      sanitizer.resolvePastedSlice(parsed, true, () => {
        throw new Error('plain-text paste must not restore rich content');
      }),
    ).toEqual(parsed);
    expect(sanitizer.takePastedSource()).toBeNull();
  });

  it('restores the source carried by this paste without controller-local state', () => {
    const parsed = new Slice(
      Fragment.from(
        schema.nodes.paragraph.create(null, schema.text('Rendered fallback')),
      ),
      0,
      0,
    );
    const restored = new Slice(
      Fragment.from(
        schema.nodes.paragraph.create(
          {
            id: 'intro',
            carveKeyValues: { role: 'summary' },
            carveAttrOrder: ['#id', 'role'],
          },
          schema.text('Canonical source'),
        ),
      ),
      0,
      0,
    );
    const sanitizer = new ClipboardPasteSanitizer();
    sanitizer.capturePastedSource('Canonical source{#intro role="summary"}');

    expect(
      sanitizer.resolvePastedSlice(parsed, false, (source) =>
        source.startsWith('Canonical source') ? restored : null,
      ),
    ).toEqual(restored);
  });

  it('preserves marks inherited from the selection on plain-text paste', () => {
    const inheritedLink = schema.marks.link.create({
      href: 'https://example.com',
      id: 'reference',
      class: 'document-link',
      carveKeyValues: { role: 'citation' },
      carveAttrOrder: ['#id', '.class', 'role'],
    });
    const heading = schema.nodes.heading.create(
      { level: 2 },
      schema.text('Plain text', [inheritedLink]),
    );

    const sanitized = new ClipboardPasteSanitizer().sanitizePastedSlice(
      new Slice(Fragment.from(heading), 0, 0),
      true,
    );

    expect(sanitized.content.firstChild?.firstChild?.marks[0]).toEqual(
      inheritedLink,
    );
  });

  it('rejects forged attribute-order metadata from external HTML', () => {
    const heading = schema.nodes.heading.create(
      {
        level: 2,
        carveKeyValues: { style: 'color: red' },
        carveAttrOrder: ['style'],
      },
      schema.text('External'),
    );

    const sanitized = new ClipboardPasteSanitizer().sanitizePastedSlice(
      new Slice(Fragment.from(heading), 0, 0),
    );

    expect(sanitized.content.firstChild?.attrs).toEqual({
      level: 2,
      id: null,
      class: null,
      carveKeyValues: null,
      carveAttrOrder: null,
    });
  });

  it('sanitizes rich HTML without a Carver source payload', () => {
    const forged = new Slice(
      Fragment.from(
        schema.nodes.heading.create(
          {
            level: 2,
            carveKeyValues: { style: 'color: red' },
            carveAttrOrder: ['style'],
          },
          schema.text('Internal'),
        ),
      ),
      0,
      0,
    );
    const sanitizer = new ClipboardPasteSanitizer();
    sanitizer.capturePastedSource(null);

    expect(
      sanitizer.resolvePastedSlice(forged, false, () => {
        throw new Error('external HTML must not use the Carver source path');
      }).content.firstChild?.attrs.carveKeyValues,
    ).toBeNull();
  });

  it('serializes pasted rich text without foreign clipboard attributes', () => {
    const carveSchema = getSchema([CarveKit]);
    const document = carveSchema.nodeFromJSON({
      type: 'doc',
      content: [
        {
          type: 'heading',
          attrs: {
            level: 2,
            class: 'ProsemirrorEditor-heading',
            carveKeyValues: {
              style: 'color: rgb(255, 255, 255)',
              'data-pm-slice': '0 0 []',
            },
            carveAttrOrder: null,
          },
          content: [
            { type: 'text', marks: [{ type: 'bold' }], text: 'Meeting notes' },
          ],
        },
      ],
    });

    const sanitized = sanitizePastedSlice(new Slice(document.content, 0, 0));
    const sanitizedDocument = carveSchema.nodes.doc.create(
      null,
      sanitized.content,
    );

    expect(serializeToCarve(sanitizedDocument.toJSON())).toBe(
      '## *Meeting notes*',
    );
  });
});
