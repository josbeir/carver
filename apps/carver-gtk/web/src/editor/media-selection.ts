import type { Node } from '@tiptap/pm/model';
import type { EditorState } from '@tiptap/pm/state';

export function mediaOccurrences(doc: Node) {
  const result: {
    path: string;
    from: number;
    to: number;
    image: boolean;
    occurrence: number;
  }[] = [];
  const counts = new Map<string, number>();
  doc.descendants((node, from) => {
    const image = node.type.name === 'image';
    const path: unknown = image
      ? node.attrs.src
      : node.isText
        ? node.marks.find((mark) => mark.type.name === 'link')?.attrs.href
        : null;
    if (typeof path !== 'string' || (!image && !path.startsWith('assets/')))
      return;
    const previous = result.at(-1);
    if (
      !image &&
      previous &&
      !previous.image &&
      previous.path === path &&
      previous.to === from
    ) {
      previous.to = from + node.nodeSize;
      return;
    }
    const occurrence = counts.get(path) ?? 0;
    counts.set(path, occurrence + 1);
    result.push({ path, from, to: from + node.nodeSize, image, occurrence });
  });
  return result;
}

export function selectedMedia(state: EditorState) {
  if (!state.selection) return null;
  const { from, to } = state.selection;
  const selected = mediaOccurrences(state.doc).find(
    (item) => item.from <= from && from < item.to && to <= item.to,
  );
  return selected
    ? { path: selected.path, occurrence: selected.occurrence }
    : null;
}
