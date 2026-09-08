import { Fragment, type Mark, type Node, Slice } from '@tiptap/pm/model';

type AttributeTransform = (
  attributes: Record<string, unknown>,
) => Record<string, unknown>;

function sanitizedAttributes(attributes: Record<string, unknown>) {
  const sanitized = { ...attributes };
  if (!('carveAttrOrder' in sanitized)) return sanitized;
  if ('id' in sanitized) sanitized.id = null;
  if ('class' in sanitized) sanitized.class = null;
  if ('carveKeyValues' in sanitized) sanitized.carveKeyValues = null;
  sanitized.carveAttrOrder = null;
  return sanitized;
}

function withoutClipboardMetadata(attributes: Record<string, unknown>) {
  const sanitized = { ...attributes };
  if (!('carveAttrOrder' in sanitized)) return sanitized;
  const keyValues = attributes.carveKeyValues;
  if (keyValues && typeof keyValues === 'object' && !Array.isArray(keyValues)) {
    const retained = Object.fromEntries(
      Object.entries(keyValues).filter(([name]) => name !== 'data-pm-slice'),
    );
    sanitized.carveKeyValues = Object.keys(retained).length ? retained : null;
  }
  const order = attributes.carveAttrOrder;
  if (Array.isArray(order)) {
    const retained = order.filter((name) => name !== 'data-pm-slice');
    sanitized.carveAttrOrder = retained.length ? retained : null;
  }
  return sanitized;
}

function transformedMark(mark: Mark, transform: AttributeTransform) {
  return mark.type.create(transform(mark.attrs));
}

function transformedNode(node: Node, transform: AttributeTransform): Node {
  const marks = node.marks.map((mark) => transformedMark(mark, transform));
  if (node.isText) return node.mark(marks);

  const children: Node[] = [];
  node.content.forEach((child) => {
    children.push(transformedNode(child, transform));
  });
  return node.type.create(
    transform(node.attrs),
    Fragment.fromArray(children),
    marks,
  );
}

function transformedSlice(slice: Slice, transform: AttributeTransform) {
  const children: Node[] = [];
  slice.content.forEach((child) => {
    children.push(transformedNode(child, transform));
  });
  return new Slice(
    Fragment.fromArray(children),
    slice.openStart,
    slice.openEnd,
  );
}

/** Removes foreign presentation attributes after clipboard HTML is parsed. */
export function sanitizePastedSlice(slice: Slice): Slice {
  return transformedSlice(slice, sanitizedAttributes);
}

/** Distinguishes exact editor-owned copies from untrusted clipboard HTML. */
export class ClipboardPasteSanitizer {
  private copiedSlice: string | null = null;

  /** Records the exact slice passed to ProseMirror's clipboard serializer. */
  public recordCopiedSlice(slice: Slice): Slice {
    this.copiedSlice = this.signature(slice);
    return slice;
  }

  /** Preserves an exact internal copy and sanitizes every other pasted slice. */
  public sanitizePastedSlice(slice: Slice): Slice {
    const content = transformedSlice(slice, withoutClipboardMetadata);
    return this.signature(content) === this.copiedSlice
      ? content
      : sanitizePastedSlice(content);
  }

  private signature(slice: Slice): string {
    return JSON.stringify(
      transformedSlice(slice, withoutClipboardMetadata).toJSON(),
    );
  }
}
