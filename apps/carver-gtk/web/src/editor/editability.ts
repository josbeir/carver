// The adapter distinguishes opaque Carve constructs from metadata it cannot
// model as a dedicated ProseMirror node. Only an opaque document must leave
// Edit mode: reported degradation such as smart punctuation or a soft break
// still has a faithful editable text representation.
export function unsupportedForEditing(result) {
  const opaque =
    result.doc?.content?.length === 1 &&
    result.doc.content[0]?.type === 'carveUnsupported';
  return opaque ? Object.keys(result.preserved ?? {}) : [];
}

// A ProseMirror Slice contains document children, not document attributes.
// Reject clipboard payloads whose exact Carve source depends on an envelope
// attached to the adapter's document node, because insertion would drop it.
export function unsupportedForPasting(result) {
  const unsupported = unsupportedForEditing(result);
  if (unsupported.length) return unsupported;
  return Object.hasOwn(result.preserved ?? {}, 'document') ? ['document'] : [];
}
