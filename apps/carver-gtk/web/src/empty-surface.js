// ProseMirror only receives pointer events within its rendered content.  For
// an empty document, extend that interaction to the whole editor surface.
export function focusEmptyEditorSurface(event, editor, root) {
  const surface = editor?.view?.dom;
  if (!editor?.isEmpty || !surface || (event.target !== root && event.target !== surface)) {
    return false;
  }

  event.preventDefault();
  editor.commands.focus('end');
  return true;
}
