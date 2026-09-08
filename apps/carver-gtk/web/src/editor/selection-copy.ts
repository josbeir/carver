import type { JSONContent } from '@tiptap/core';
import type { Node } from '@tiptap/pm/model';
import { TextSelection, type EditorState } from '@tiptap/pm/state';
import { serializeToCarve } from '@markup-carve/carve-grammars/tiptap';

function nodeContent(nodes: readonly Node[]): JSONContent[] {
  return nodes.map((node) => node.toJSON() as JSONContent);
}

/** Serializes a non-empty ProseMirror selection as standalone canonical Carve. */
export function selectedCarveSource(state: EditorState): string | null {
  const { selection } = state;
  if (selection.empty) return null;

  if (
    selection instanceof TextSelection &&
    selection.$from.sameParent(selection.$to) &&
    selection.$from.parent.isTextblock
  ) {
    const selectedBlock = selection.$from.parent.cut(
      selection.$from.parentOffset,
      selection.$to.parentOffset,
    );
    return serializeToCarve({ type: 'doc', content: [selectedBlock.toJSON()] });
  }

  const slice = selection.content();
  const nodes: Node[] = [];
  slice.content.forEach((node) => {
    nodes.push(node);
  });
  if (!nodes.length) return null;

  const content = nodes[0]?.isInline
    ? [{ type: 'paragraph', content: nodeContent(nodes) }]
    : nodeContent(nodes);
  return serializeToCarve({ type: 'doc', content });
}
