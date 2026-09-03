import { Editor, mergeAttributes } from '@tiptap/core';
import Image from '@tiptap/extension-image';
import './style.css';
import { unsupportedForEditing } from './editability.js';
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
    ['superscript', active('superscript')], ['subscript', active('subscript')],
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
    case 'inline-code': return chain.toggleCode().run();
    case 'highlight': return chain.toggleHighlight().run();
    case 'superscript': return chain.toggleSuperscript().run();
    case 'subscript': return chain.toggleSubscript().run();
    case 'bullet-list': return chain.toggleBulletList().run();
    case 'ordered-list': return chain.toggleOrderedList().run();
    case 'task-list': return chain.toggleTaskList().run();
    case 'code-block': return chain.toggleCodeBlock().run();
    case 'heading': return argument ? chain.toggleHeading({ level: argument }).run() : chain.setParagraph().run();
    case 'insert-table': return chain.insertTable({ rows: argument?.rows ?? 3, cols: argument?.columns ?? 3, withHeaderRow: argument?.header ?? true }).run();
    case 'add-row-before': return chain.addRowBefore().run();
    case 'add-row-after': return chain.addRowAfter().run();
    case 'delete-row': return chain.deleteRow().run();
    case 'add-column-before': return chain.addColumnBefore().run();
    case 'add-column-after': return chain.addColumnAfter().run();
    case 'delete-column': return chain.deleteColumn().run();
    case 'delete-table': return chain.deleteTable().run();
    case 'image-width': return setImageWidth(argument);
    case 'undo': return chain.undo().run();
    case 'redo': return chain.redo().run();
    default: return false;
  }
}

function setImageWidth(width) {
  if (!editor.isActive('image')) return false;
  const attrs = editor.getAttributes('image');
  const values = { ...(attrs.carveKeyValues ?? {}) };
  if (width) values.width = `${width}%`;
  else delete values.width;
  return editor.chain().focus().updateAttributes('image', { carveKeyValues: values }).run();
}

function pasteImage(event) {
  const item = [...(event.clipboardData?.items ?? [])].find(item => item.type.startsWith('image/'));
  if (!item) return;
  event.preventDefault();
  const file = item.getAsFile();
  if (!file) return;
  const reader = new FileReader();
  reader.onload = () => {
    const data = String(reader.result).split(',', 2)[1];
    send({ type: 'paste-image', session, mime_type: file.type, data });
  };
  reader.readAsDataURL(file);
  return true;
}

function insertImage(path) {
  if (replacePendingBlobImage(path)) return;
  editor.chain().focus().insertContent([
    { type: 'paragraph', content: [{ type: 'image', attrs: { src: path, alt: 'Pasted image' } }] },
    { type: 'paragraph' },
  ]).run();
}

function replacePendingBlobImage(path) {
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
    alt: pending.attrs.alt || 'Pasted image',
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
    editorProps: { handlePaste: (_view, event) => pasteImage(event) ?? false },
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
  send({ type: 'ready' });
}

globalThis.carverEditor = {
  load,
  command,
  source: () => editor ? serializeToCarve(editor.getJSON()) : '',
  insertImage,
  setTheme: (dark, selectionBackground, selectionForeground) => {
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
    document.documentElement.style.setProperty('--selection-background', selectionBackground);
    document.documentElement.style.setProperty('--selection-foreground', selectionForeground);
  },
};

initialize();
