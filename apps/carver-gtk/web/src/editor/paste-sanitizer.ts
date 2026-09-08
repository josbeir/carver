import { Fragment, type Mark, type Node, Slice } from '@tiptap/pm/model';

function sanitizedAttributes(attributes: Record<string, unknown>) {
  const sanitized = { ...attributes };
  if (!('carveAttrOrder' in sanitized)) return sanitized;

  const order = Array.isArray(attributes.carveAttrOrder)
    ? attributes.carveAttrOrder.filter(
        (slot): slot is string => typeof slot === 'string',
      )
    : [];
  const authoredSlots = new Set(order);

  if ('id' in sanitized && !authoredSlots.has('#id')) sanitized.id = null;
  if ('class' in sanitized && !authoredSlots.has('.class')) {
    sanitized.class = null;
  }

  const keyValues = attributes.carveKeyValues;
  if (
    'carveKeyValues' in sanitized &&
    keyValues &&
    typeof keyValues === 'object' &&
    !Array.isArray(keyValues)
  ) {
    const authoredKeyValues = Object.fromEntries(
      Object.entries(keyValues).filter(([name]) => authoredSlots.has(name)),
    );
    sanitized.carveKeyValues = Object.keys(authoredKeyValues).length
      ? authoredKeyValues
      : null;
  }

  const retainedOrder = order.filter((slot) => {
    if (slot === '#id') return sanitized.id != null;
    if (slot === '.class') return sanitized.class != null;
    const retainedKeyValues = sanitized.carveKeyValues;
    return (
      retainedKeyValues != null &&
      typeof retainedKeyValues === 'object' &&
      Object.hasOwn(retainedKeyValues, slot)
    );
  });
  sanitized.carveAttrOrder = retainedOrder.length ? retainedOrder : null;
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
