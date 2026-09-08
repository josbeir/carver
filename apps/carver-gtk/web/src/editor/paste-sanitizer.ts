import { Fragment, type Mark, type Node, Slice } from '@tiptap/pm/model';

type AttributeTransform = (
  attributes: Record<string, unknown>,
) => Record<string, unknown>;

export const INTERNAL_COPY_MARKER = '<!--carver-internal-copy-->';

function sanitizedAttributes(attributes: Record<string, unknown>) {
  const sanitized = { ...attributes };
  if (!('carveAttrOrder' in sanitized)) return sanitized;
  if ('id' in sanitized) sanitized.id = null;
  if ('class' in sanitized) sanitized.class = null;
  if ('carveKeyValues' in sanitized) sanitized.carveKeyValues = null;
  sanitized.carveAttrOrder = null;
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
  private copiedSlice: Slice | null = null;
  private pasteIsInternal = false;

  /** Records the exact slice passed to ProseMirror's clipboard serializer. */
  public recordCopiedSlice(slice: Slice): Slice {
    this.copiedSlice = slice;
    return slice;
  }

  /** Recognizes and removes the native selection marker before HTML is parsed. */
  public preparePastedHtml(html: string): string {
    this.pasteIsInternal =
      this.copiedSlice !== null && html.includes(INTERNAL_COPY_MARKER);
    return html.replace(INTERNAL_COPY_MARKER, '');
  }

  /** Preserves plain-text context or an editor-owned copy; sanitizes rich text. */
  public sanitizePastedSlice(slice: Slice, plain = false): Slice {
    const copiedSlice = this.pasteIsInternal ? this.copiedSlice : null;
    this.pasteIsInternal = false;
    if (plain) return slice;
    return copiedSlice ?? sanitizePastedSlice(slice);
  }
}
