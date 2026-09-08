import { Fragment, type Mark, type Node, Slice } from '@tiptap/pm/model';

const PASTED_ATTRIBUTE_SLOTS = [
  'id',
  'class',
  'carveKeyValues',
  'carveAttrOrder',
] as const;

function sanitizedAttributes(attributes: Record<string, unknown>) {
  const sanitized = { ...attributes };
  for (const name of PASTED_ATTRIBUTE_SLOTS) {
    if (name in sanitized) sanitized[name] = null;
  }
  return sanitized;
}

function sanitizedMark(mark: Mark) {
  return mark.type.create(sanitizedAttributes(mark.attrs));
}

function sanitizedNode(node: Node): Node {
  const marks = node.marks.map(sanitizedMark);
  if (node.isText) return node.mark(marks);

  const children: Node[] = [];
  node.content.forEach((child) => {
    children.push(sanitizedNode(child));
  });
  return node.type.create(
    sanitizedAttributes(node.attrs),
    Fragment.fromArray(children),
    marks,
  );
}

/** Removes foreign presentation attributes after clipboard HTML is parsed. */
export function sanitizePastedSlice(slice: Slice): Slice {
  const children: Node[] = [];
  slice.content.forEach((child) => {
    children.push(sanitizedNode(child));
  });
  return new Slice(
    Fragment.fromArray(children),
    slice.openStart,
    slice.openEnd,
  );
}
