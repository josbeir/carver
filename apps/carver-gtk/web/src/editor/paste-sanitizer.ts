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
  private pastedSource: string | null = null;

  /** Captures canonical Carve from the native clipboard format before parsing. */
  public capturePastedSource(source: string | null): void {
    this.pastedSource = source;
  }

  /** Consumes canonical Carve carried by the current native clipboard paste. */
  public takePastedSource(): string | null {
    const source = this.pastedSource;
    this.pastedSource = null;
    return source;
  }

  /** Preserves plain-text context and sanitizes foreign rich text. */
  public sanitizePastedSlice(slice: Slice, plain = false): Slice {
    return plain ? slice : sanitizePastedSlice(slice);
  }

  /** Restores typed Carver content or refuses an unsupported internal paste. */
  public resolvePastedSlice(
    slice: Slice,
    plain: boolean,
    restore: (source: string) => Slice | null,
  ): Slice {
    const source = this.takePastedSource();
    if (plain) return slice;
    if (source !== null) return restore(source) ?? Slice.empty;
    return sanitizePastedSlice(slice);
  }
}
