import type { Mark, Node } from '@tiptap/pm/model';
import type { EditorState } from '@tiptap/pm/state';

type MediaOccurrence = {
  path: string;
  from: number;
  to: number;
  image: boolean;
  occurrence: number;
};

type PendingMediaOccurrence = MediaOccurrence & { link?: Mark };

export function mediaOccurrences(doc: Node) {
  const result: PendingMediaOccurrence[] = [];
  const counts = new Map<string, number>();
  doc.descendants((node, from) => {
    const image = node.type.name === 'image';
    const link = node.isText
      ? node.marks.find((mark) => mark.type.name === 'link')
      : undefined;
    const path: unknown = image ? node.attrs.src : link?.attrs.href;
    if (typeof path !== 'string' || (!image && !path.startsWith('assets/')))
      return;
    const previous = result.at(-1);
    if (
      !image &&
      previous &&
      !previous.image &&
      previous.path === path &&
      previous.to === from &&
      previous.link?.eq(link)
    ) {
      previous.to = from + node.nodeSize;
      return;
    }
    const occurrence = counts.get(path) ?? 0;
    counts.set(path, occurrence + 1);
    result.push({
      path,
      from,
      to: from + node.nodeSize,
      image,
      occurrence,
      link,
    });
  });
  return result.map(({ link: _link, ...occurrence }) => occurrence);
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
