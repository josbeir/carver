import { Fragment, type Mark, type Node, Slice } from '@tiptap/pm/model';

type AttributeTransform = (
  attributes: Record<string, unknown>,
) => Record<string, unknown>;

const COPY_MARKER_PREFIX = 'carver-copy:';

function createCopyMarker(): string {
  const values = new Uint32Array(4);
  globalThis.crypto.getRandomValues(values);
  return Array.from(values, (value) =>
    value.toString(16).padStart(8, '0'),
  ).join('');
}

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
  private copyMarker: string | null = null;
  private pasteIsInternal = false;

  public constructor(
    private readonly createMarker: () => string = createCopyMarker,
  ) {}

  /** Records the exact slice passed to ProseMirror's clipboard serializer. */
  public recordCopiedSlice(slice: Slice): Slice {
    this.copiedSlice = slice;
    this.copyMarker = this.createMarker();
    return slice;
  }

  /** Returns the private marker to append to editor-owned clipboard HTML. */
  public copiedHtmlMarker(): string | null {
    return this.copyMarker ? `${COPY_MARKER_PREFIX}${this.copyMarker}` : null;
  }

  /** Recognizes and removes the private marker before clipboard HTML is parsed. */
  public preparePastedHtml(html: string): string {
    const marker = this.copiedHtmlMarker();
    const comment = marker ? `<!--${marker}-->` : null;
    this.pasteIsInternal = comment !== null && html.includes(comment);
    return comment ? html.replace(comment, '') : html;
  }

  /** Preserves plain-text context or an editor-owned copy; sanitizes rich text. */
  public sanitizePastedSlice(slice: Slice, plain = false): Slice {
    const copiedSlice = this.pasteIsInternal ? this.copiedSlice : null;
    this.pasteIsInternal = false;
    if (plain) return slice;
    return copiedSlice ?? sanitizePastedSlice(slice);
  }
}
