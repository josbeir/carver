import { Fragment, type Mark, type Node, Slice } from '@tiptap/pm/model';

type AttributeTransform = (
  attributes: Record<string, unknown>,
) => Record<string, unknown>;

const INTERNAL_COPY_PATTERN = /<!--carver-source:([A-Za-z0-9+/]*={0,2})-->/;

function decodedSource(encoded: string): string | null {
  try {
    const bytes = Uint8Array.from(globalThis.atob(encoded), (value) =>
      value.charCodeAt(0),
    );
    return new TextDecoder().decode(bytes);
  } catch {
    return null;
  }
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
  private pastedSource: string | null = null;

  /** Extracts canonical Carve from native clipboard HTML before it is parsed. */
  public preparePastedHtml(html: string): string {
    const match = INTERNAL_COPY_PATTERN.exec(html);
    this.pastedSource = match ? decodedSource(match[1]) : null;
    return match ? html.replace(match[0], '') : html;
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
}
