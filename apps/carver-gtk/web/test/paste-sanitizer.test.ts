import { Fragment, Schema, Slice } from '@tiptap/pm/model';
import { getSchema } from '@tiptap/core';
import { describe, expect, it } from 'vitest';

import {
  CarveKit,
  serializeToCarve,
} from '@markup-carve/carve-grammars/tiptap';
import { sanitizePastedSlice } from '../src/editor/paste-sanitizer';

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
    text: { group: 'inline' },
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
        carveAttrOrder: ['style', 'data-pm-slice'],
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
      carveAttrOrder: ['style'],
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
            carveAttrOrder: ['style', 'data-pm-slice'],
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
