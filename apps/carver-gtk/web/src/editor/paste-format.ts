import { Fragment, type Schema, Slice } from '@tiptap/pm/model';

// A cheap pre-check that decides whether plain clipboard text is worth asking
// the native host about. It deliberately over-approximates: the host owns the
// authoritative Carve/Markdown decision, and a false positive only costs one
// local round trip. Ordinary prose without any markup characters never leaves
// the browser's native paste path.
const BLOCK_START = /^\s*(?:#{1,6}\s|>\s?|[-+*]\s|\d+[.)]\s|::\s|\|)/m;
const INLINE_DELIMITER = /[*_~=`^]/;
const LINK_OR_ATTRIBUTE = /\[[^\]]*\]\(|\[[^\]]*\]\{|\{[^}]*\}/;
const MULTI_BLOCK = /\n\s*\n/;

/** Returns whether plain text may carry Carve or Markdown markup. */
export function mightBeMarkupText(text: string): boolean {
  return (
    BLOCK_START.test(text) ||
    INLINE_DELIMITER.test(text) ||
    LINK_OR_ATTRIBUTE.test(text) ||
    MULTI_BLOCK.test(text)
  );
}

/**
 * Builds a faithful plain-text slice.
 *
 * Used when the host classifies pasted text as unstructured, so paragraph
 * breaks and in-paragraph newlines survive without reinterpretation.
 */
export function plainTextSlice(schema: Schema, text: string): Slice {
  const normalized = text.replace(/\r\n?/g, '\n');
  const hardBreak = schema.nodes.hardBreak;
  const paragraphs = normalized.split(/\n{2,}/).map((block) => {
    const inline = [];
    block.split('\n').forEach((line, index) => {
      if (index > 0 && hardBreak) inline.push(hardBreak.create());
      if (line.length) inline.push(schema.text(line));
    });
    return schema.nodes.paragraph.create(null, inline);
  });
  return Slice.maxOpen(Fragment.fromArray(paragraphs));
}
