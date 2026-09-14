import { describe, expect, it } from 'vitest';

import {
  frontmatterFieldValue,
  frontmatterFieldsAreEditable,
  setFrontmatterField,
} from '../src/editor/frontmatter-fields';

describe('frontmatter field editing', () => {
  it('reads a title from a JSON object', () => {
    expect(frontmatterFieldValue('{"title":"Current"}', 'title', 'json')).toBe(
      'Current',
    );
  });

  it('updates JSON fields without changing the document shape', () => {
    const source = '{"title":"Current","priority":4}';

    expect(
      JSON.parse(setFrontmatterField(source, 'title', 'Updated', 'json')),
    ).toEqual({
      title: 'Updated',
      priority: 4,
    });
  });

  it('does not modify malformed JSON through a field control', () => {
    const source = '{"title":"Current"';

    expect(setFrontmatterField(source, 'title', 'Updated', 'json')).toBe(
      source,
    );
  });

  it('disables field controls for malformed JSON', () => {
    expect(frontmatterFieldsAreEditable('{"title":"Current"', 'json')).toBe(
      false,
    );
  });

  it('keeps field controls available for JSON objects and YAML', () => {
    expect(frontmatterFieldsAreEditable('{"title":"Current"}', 'json')).toBe(
      true,
    );
    expect(frontmatterFieldsAreEditable('title: Current', 'yaml')).toBe(true);
  });

  it('reads quoted TOML and YAML values', () => {
    expect(frontmatterFieldValue('title = "Current"', 'title', 'toml')).toBe(
      'Current',
    );
    expect(frontmatterFieldValue("title: 'Current'", 'title', 'yaml')).toBe(
      'Current',
    );
  });

  it('writes and removes YAML metadata fields', () => {
    expect(
      setFrontmatterField('title: Current\n', 'title', 'Updated', 'yaml'),
    ).toBe('title: "Updated"\n');
    expect(setFrontmatterField('title: Current\n', 'title', '', 'yaml')).toBe(
      '',
    );
  });

  it('adds TOML metadata before its first table', () => {
    expect(
      setFrontmatterField('[owner]\nname = "Ada"', 'title', 'Current', 'toml'),
    ).toBe('title = "Current"\n[owner]\nname = "Ada"');
  });
});
