import { getMarkRange } from '@tiptap/core';

function activeLinkRange(state) {
  const link = state.schema.marks.link;
  return link ? getMarkRange(state.selection.$from, link) : undefined;
}

function linkDestination(state, from) {
  const link = state.schema.marks.link;
  return state.doc.nodeAt(from)?.marks.find(mark => mark.type === link)?.attrs.href ?? '';
}

export function linkContext(state) {
  const range = activeLinkRange(state);
  if (!range) {
    const { from, to } = state.selection;
    return { text: state.doc.textBetween(from, to, ' '), destination: '' };
  }
  return {
    text: state.doc.textBetween(range.from, range.to, ' '),
    destination: linkDestination(state, range.from),
  };
}

export function insertOrUpdateLink(state, argument) {
  const text = typeof argument?.text === 'string' ? argument.text : '';
  const href = typeof argument?.destination === 'string' ? argument.destination.trim() : '';
  const link = state.schema.marks.link;
  if (!text.trim() || !href || !link) return undefined;

  const range = activeLinkRange(state);
  const { from, to } = range ?? state.selection;
  return state.tr
    .removeMark(from, to, link)
    .insertText(text, from, to)
    .addMark(from, from + text.length, link.create({ href }));
}
