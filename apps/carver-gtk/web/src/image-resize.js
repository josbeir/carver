import { NodeSelection } from '@tiptap/pm/state';

// Attribute changes map a node selection but do not guarantee that it remains
// a node selection. Keep the image selected so consecutive toolbar resizes
// continue to target the same image.
export function resizeSelectedImage(state, width) {
  const { selection } = state;
  if (!(selection instanceof NodeSelection) || selection.node.type.name !== 'image') {
    return null;
  }

  const values = { ...(selection.node.attrs.carveKeyValues ?? {}) };
  if (width) values.width = `${width}%`;
  else delete values.width;

  const transaction = state.tr.setNodeMarkup(selection.from, undefined, {
    ...selection.node.attrs,
    carveKeyValues: values,
  });
  const imagePosition = transaction.mapping.map(selection.from);
  return transaction.setSelection(NodeSelection.create(transaction.doc, imagePosition));
}
