// The adapter distinguishes opaque Carve constructs from metadata it cannot
// model as a dedicated ProseMirror node. Only an opaque document must leave
// Edit mode: reported degradation such as smart punctuation or a soft break
// still has a faithful editable text representation.
export function unsupportedForEditing(result) {
  const opaque = result.doc?.content?.length === 1
    && result.doc.content[0]?.type === 'carveUnsupported';
  return opaque ? Object.keys(result.preserved ?? {}) : [];
}
