import { Editor, mergeAttributes } from '@tiptap/core';
import Image from '@tiptap/extension-image';
import './style.css';
import { unsupportedForEditing } from './editability.js';
import { focusEmptyEditorSurface } from './empty-surface.js';
import { insertOrUpdateLink, linkContext } from './link.js';
import { resizeSelectedImage } from './image-resize.js';
import { resizeSelectedTable, tableSize } from './table-resize.js';
import {
  CarveKit,
  carveToProseMirrorWithReport,
  serializeToCarve,
} from '@markup-carve/carve-grammars/tiptap';

const host = globalThis.webkit?.messageHandlers?.carver;
const root = document.getElementById('editor');
let editor = null;
let loading = false;
let session = 0;
let revision = 0;
const pendingBlobSources = new Set();

// Carve stores presentation attributes in `carveKeyValues`. Tiptap's stock
// image node has no knowledge of those attributes, so mirror its schema while
// projecting a supported width onto the DOM. Source serialization still uses
// `carveKeyValues`, never a browser-only pixel width.
const CarveImage = Image.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      carveRef: { default: null, renderHTML: () => ({}) },
      carveRawRef: { default: null, renderHTML: () => ({}) },
      id: { default: null, renderHTML: attributes => attributes.id ? { id: attributes.id } : {} },
      class: { default: null, renderHTML: attributes => attributes.class ? { class: attributes.class } : {} },
      carveKeyValues: { default: null, renderHTML: () => ({}) },
      carveAttrOrder: { default: null, renderHTML: () => ({}) },
    };
  },
  renderHTML({ node, HTMLAttributes }) {
    const width = node.attrs.carveKeyValues?.width;
    const style = typeof width === 'string' && /^\d{1,3}%$/.test(width)
      ? `width: ${width};`
      : null;
    return ['img', mergeAttributes(HTMLAttributes, style ? { style } : {})];
  },
}).configure({ inline: true });

function send(event) {
  host?.postMessage(JSON.stringify(event));
}

function reportSelection() {
  if (!editor || loading) return;
  const active = (name, attrs) => editor.isActive(name, attrs);
  const attrs = editor.getAttributes('heading');
  const image = active('image');
  const states = [
    ['bold', active('bold')], ['italic', active('italic')], ['strike', active('strike')],
    ['underline', active('underline')], ['inline-code', active('code')], ['highlight', active('highlight')],
    ['superscript', active('superscript')], ['subscript', active('subscript')], ['link', active('link')],
    ['bullet-list', active('bulletList')], ['ordered-list', active('orderedList')], ['task-list', active('taskList')],
    ['code-block', active('codeBlock')], ['table', active('table')], ['image', image],
  ];
  send({
    type: 'selection',
    session,
    state: {
      active: states.filter(([, enabled]) => enabled).map(([name]) => name),
      heading: active('heading') ? attrs.level : 0,
      image_width: image ? imageWidth() : null,
    },
  });
}

function imageWidth() {
  const attrs = editor.getAttributes('image');
  const value = attrs.carveKeyValues?.width;
  const match = typeof value === 'string' && /^(\d{1,3})%$/.exec(value);
  return match ? Number(match[1]) : 0;
}

function load(source, nextSession) {
  session = nextSession;
  revision = 0;
  const result = carveToProseMirrorWithReport(source, { unsupported: 'preserve' });
  const unsupported = unsupportedForEditing(result);
  if (unsupported.length) {
    send({ type: 'unsupported', session, unsupported, degraded: [] });
    return false;
  }
  loading = true;
  try {
    editor.commands.setContent(result.doc);
  } finally {
    loading = false;
  }
  reportSelection();
  return true;
}

function command(name, argument) {
  const chain = editor.chain().focus();
  switch (name) {
    case 'bold': return chain.toggleBold().run();
    case 'italic': return chain.toggleItalic().run();
    case 'strike': return chain.toggleStrike().run();
    case 'underline': return chain.toggleUnderline().run();
    case 'highlight': return chain.toggleHighlight().run();
    case 'superscript': return chain.toggleSuperscript().run();
    case 'subscript': return chain.toggleSubscript().run();
    case 'inline-code': return chain.toggleCode().run();
    case 'bullet-list': return chain.toggleBulletList().run();
    case 'ordered-list': return chain.toggleOrderedList().run();
    case 'task-list': return chain.toggleTaskList().run();
    case 'code-block': return chain.toggleCodeBlock().run();
    case 'heading': return argument ? chain.toggleHeading({ level: argument }).run() : chain.setParagraph().run();
    case 'insert-table': return insertOrResizeTable(argument);
    case 'image-width': return setImageWidth(argument);
    case 'insert-link': return insertLink(argument);
    case 'undo': return chain.undo().run();
    case 'redo': return chain.redo().run();
    default: return false;
  }
}

function insertOrResizeTable(argument) {
  // The table picker is native GTK UI, so using it moves focus away from the
  // WebView. Restore the editor selection before asking Tiptap which table is
  // active; otherwise a later resize is treated as an insertion attempt.
  editor.commands.focus();
  const { rows, columns } = tableSize(argument?.rows, argument?.columns);
  const header = argument?.header ?? true;
  if (!editor.isActive('table')) {
    return editor.chain().focus().insertTable({ rows, cols: columns, withHeaderRow: header }).run();
  }
  const transactions = resizeSelectedTable(editor.state, { rows, columns, header });
  if (!transactions.length) return false;
  transactions.forEach(transaction => editor.view.dispatch(transaction.scrollIntoView()));
  return true;
}

function insertLink(argument) {
  editor.commands.focus();
  const transaction = insertOrUpdateLink(editor.state, argument);
  if (!transaction) return false;
  editor.view.dispatch(transaction);
  return true;
}

function setImageWidth(width) {
  const transaction = resizeSelectedImage(editor.state, width);
  if (!transaction) return false;
  editor.view.dispatch(transaction);
  editor.commands.focus();
  return true;
}

function pasteImage(event) {
  const item = [...(event.clipboardData?.items ?? [])].find(item => item.type.startsWith('image/'));
  if (!item) return;
  event.preventDefault();
  const file = item.getAsFile();
  if (!file) return false;
  return persistImageFile(file);
}

function persistImageFile(file) {
  if (!file?.type.startsWith('image/')) return false;
  const reader = new FileReader();
  reader.onload = () => {
    const data = String(reader.result).split(',', 2)[1];
    send({ type: 'paste-image', session, mime_type: file.type, data });
  };
  reader.readAsDataURL(file);
  return true;
}

function dropImages(event) {
  const files = [...(event.dataTransfer?.files ?? [])].filter(file => file.type.startsWith('image/'));
  if (!files.length) {
    // WebKitGTK commonly reports a native image drop as a file URI rather
    // than a browser File. The GTK drop target imports that file; suppress
    // WebKit's default URI-text insertion in the meantime.
    const uriList = event.dataTransfer?.getData('text/uri-list') ?? '';
    if (!/^file:\/\/\/.+\.(?:avif|gif|jpe?g|png|svg|webp)(?:\r?$)/im.test(uriList)) {
      return false;
    }
    event.preventDefault();
    return true;
  }
  event.preventDefault();
  const position = editor.view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos;
  if (position != null) editor.chain().focus().setTextSelection(position).run();
  files.forEach(persistImageFile);
  return true;
}

function insertImage(path, alt = 'Pasted image') {
  if (replacePendingBlobImage(path, alt)) return;
  editor.chain().focus().insertContent([
    { type: 'paragraph', content: [{ type: 'image', attrs: { src: path, alt } }] },
    { type: 'paragraph' },
  ]).run();
}

function replacePendingBlobImage(path, alt) {
  let pending = null;
  editor.state.doc.descendants((node, position) => {
    if (node.type.name === 'image' && typeof node.attrs.src === 'string'
      && node.attrs.src.startsWith('blob:') && pendingBlobSources.has(node.attrs.src)) {
      pending = { position, attrs: node.attrs };
      return false;
    }
    return true;
  });
  if (!pending) return false;

  pendingBlobSources.delete(pending.attrs.src);
  return editor.chain().setNodeSelection(pending.position).updateAttributes('image', {
    ...pending.attrs,
    src: path,
    alt: pending.attrs.alt || alt,
  }).run();
}

function persistUnexpectedBlobImages() {
  let found = false;
  editor.state.doc.descendants(node => {
    const source = node.type.name === 'image' ? node.attrs.src : null;
    if (typeof source !== 'string' || !source.startsWith('blob:')) return true;
    found = true;
    persistBlobImage(source);
    return true;
  });
  return found;
}

function persistBlobImage(source) {
  if (pendingBlobSources.has(source)) return;
  pendingBlobSources.add(source);
  fetch(source)
    .then(response => response.ok ? response.blob() : Promise.reject(new Error('blob unavailable')))
    .then(blob => new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve({
        data: String(reader.result).split(',', 2)[1],
        mimeType: blob.type || 'image/png',
      });
      reader.onerror = reject;
      reader.readAsDataURL(blob);
    }))
    .then(({ data, mimeType }) => send({ type: 'paste-image', session, mime_type: mimeType, data }))
    .catch(() => pendingBlobSources.delete(source));
}

function initialize() {
  editor = new Editor({
    element: root,
    extensions: [CarveKit.configure({ image: false }), CarveImage],
    content: { type: 'doc', content: [{ type: 'paragraph' }] },
    editorProps: {
      handlePaste: (_view, event) => pasteImage(event) ?? false,
      handleDrop: (_view, event) => dropImages(event),
    },
    onUpdate: ({ editor: updated }) => {
      if (loading) return;
      // WebKit may turn a clipboard image into a `blob:` image before it
      // dispatches a paste event. Never let that ephemeral URL reach stored
      // Carve; persist and replace the node through the asset bridge instead.
      if (persistUnexpectedBlobImages()) return;
      revision += 1;
      send({ type: 'changed', session, revision, source: serializeToCarve(updated.getJSON()) });
    },
    onSelectionUpdate: reportSelection,
    onTransaction: reportSelection,
  });
  // ProseMirror's handler is normally sufficient, but WebKit can insert an
  // image before a bubbling editor handler runs. Capture it at the root so a
  // clipboard file is always stored by the native asset backend first.
  root.addEventListener('paste', event => {
    if (pasteImage(event)) event.stopPropagation();
  }, true);
  root.addEventListener('drop', event => {
    if (dropImages(event)) event.stopPropagation();
  }, true);
  root.addEventListener('pointerdown', event => {
    focusEmptyEditorSurface(event, editor, root);
  });
  send({ type: 'ready' });
}

globalThis.carverEditor = {
  load,
  command,
  source: () => editor ? serializeToCarve(editor.getJSON()) : '',
  linkContext: () => editor ? linkContext(editor.state) : { text: '', destination: '' },
  insertImage,
  setTheme: (dark, accent, selectionBackground, selectionForeground) => {
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
    document.documentElement.style.setProperty('--accent-color', accent);
    document.documentElement.style.setProperty('--selection-background', selectionBackground);
    document.documentElement.style.setProperty('--selection-foreground', selectionForeground);
  },
};

initialize();
