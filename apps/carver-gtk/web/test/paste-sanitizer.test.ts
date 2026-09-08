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

  it('extracts canonical source from Carver clipboard HTML', () => {
    const source = '## R\u00e9sum\u00e9{#references role="bibliography"}';
    const encoded = globalThis.btoa(
      String.fromCharCode(...new TextEncoder().encode(source)),
    );
    const sanitizer = new ClipboardPasteSanitizer();

    expect(
      sanitizer.preparePastedHtml(
        `<h2>R\u00e9sum\u00e9</h2><!--carver-source:${encoded}-->`,
      ),
    ).toBe('<h2>R\u00e9sum\u00e9</h2>');
    expect(sanitizer.takePastedSource()).toBe(source);
    expect(sanitizer.takePastedSource()).toBeNull();
  });

  it('ignores malformed Carver clipboard payloads', () => {
    const sanitizer = new ClipboardPasteSanitizer();

    expect(
      sanitizer.preparePastedHtml('<p>External</p><!--carver-source:a-->'),
    ).toBe('<p>External</p>');
    expect(sanitizer.takePastedSource()).toBeNull();
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
    sanitizer.preparePastedHtml('<h2>Internal</h2>');
    expect(sanitizer.takePastedSource()).toBeNull();

    expect(
      sanitizer.sanitizePastedSlice(forged).content.firstChild?.attrs
        .carveKeyValues,
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
