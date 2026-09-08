import type { Node } from '@tiptap/pm/model';
import type { EditorState } from '@tiptap/pm/state';

/** Maps projection headings in the same authored order as the Carve AST. */
export function headingOccurrences(doc: Node) {
  const headings: { from: number; to: number }[] = [];
  doc.descendants((node, position) => {
    if (node.type.name === 'heading') {
      headings.push({ from: position + 1, to: position + node.nodeSize - 1 });
    }
  });
  return headings;
}

/** Resolves only a selection contained in a heading, never its following section. */
export function selectedHeading(state: EditorState): number | null {
  const { from, to } = state.selection;
  const index = headingOccurrences(state.doc).findIndex(
    (heading) => heading.from <= from && to <= heading.to,
  );
  return index < 0 ? null : index;
}
