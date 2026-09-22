import { Editor, mergeAttributes } from '@tiptap/core';
import Image from '@tiptap/extension-image';
import { Slice } from '@tiptap/pm/model';
import { mediaOccurrences, selectedMedia } from './media-selection';
import { headingOccurrences, selectedHeading } from './heading-selection';
import {
  CarveKit,
  carveToProseMirrorWithReport,
  serializeToCarve,
} from '@markup-carve/carve-grammars/tiptap';
import { unsupportedForEditing, unsupportedForPasting } from './editability';
import { focusEmptyEditorSurface } from './empty-surface';
import { resizeSelectedImage } from './image-resize';
import { insertOrUpdateLink, linkContext } from './link';
import { ClipboardPasteSanitizer } from './paste-sanitizer';
import { mightBeMarkupText, plainTextSlice } from './paste-format';
import { selectedCarveSource } from './selection-copy';
import type {
  DocumentTarget,
  EditorEvent,
  LinkCommand,
  LinkContext,
  RichEditorApi,
  TableCommand,
  WebKitMessageHandler,
} from './protocol';
import { resizeSelectedTable, tableSize } from './table-resize';

// Tiptap extends this API at runtime as extensions are registered. Keep that
// dynamic boundary inside the controller; protocol and helper modules remain
// fully typed and independently testable.
// biome-ignore lint/suspicious/noExplicitAny: Tiptap augments its command API at runtime.
type RuntimeEditor = any;
type EditorFactory = (options: Record<string, unknown>) => RuntimeEditor;
const CARVER_CLIPBOARD_TYPE = 'application/x-carver-source';
const RUNTIME_STYLES_ID = 'editor-runtime-styles';

const CarveImage = Image.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      carveRef: { default: null, renderHTML: () => ({}) },
      carveRawRef: { default: null, renderHTML: () => ({}) },
      id: {
        default: null,
        renderHTML: (attributes) =>
          attributes.id ? { id: attributes.id } : {},
      },
      class: {
        default: null,
        renderHTML: (attributes) =>
          attributes.class ? { class: attributes.class } : {},
      },
      carveKeyValues: { default: null, renderHTML: () => ({}) },
      carveAttrOrder: { default: null, renderHTML: () => ({}) },
    };
  },
  renderHTML({ node, HTMLAttributes }) {
    const width = node.attrs.carveKeyValues?.width;
    const style =
      typeof width === 'string' && /^\d{1,3}%$/.test(width)
        ? `width: ${width};`
        : null;
    return ['img', mergeAttributes(HTMLAttributes, style ? { style } : {})];
  },
}).configure({ inline: true });

/** Owns the mutable WebKit/Tiptap editor lifecycle for one editor document. */
export class EditorController implements RichEditorApi {
  private editor: RuntimeEditor | null = null;
  private loading = false;
  private session = 0;
  private revision = 0;
  private navigationEpoch = 0;
  private navigating = false;
  private themeStyles = '';
  private appearanceStyles = '';
  private readonly pendingBlobSources = new Set<string>();
  private readonly pasteSanitizer = new ClipboardPasteSanitizer();
  // Replies can arrive out of order with concurrent pastes, so each request
  // keeps its own target and is matched by request id. The user-edit epoch lets
  // our own inserts advance the projection without invalidating their siblings.
  private readonly pendingPastes: Array<{
    requestId: number;
    session: number;
    epoch: number;
    from: number;
    to: number;
  }> = [];
  private nextPasteId = 0;
  private userEditEpoch = 0;
  private applyingPaste = false;

  public constructor(
    private readonly root: HTMLElement,
    private readonly host?: WebKitMessageHandler,
    private readonly createEditor: EditorFactory = (options) =>
      new Editor(options),
  ) {}

  /** Creates Tiptap and attaches browser event handlers exactly once. */
  public initialize(): void {
    this.editor = this.createEditor({
      element: this.root,
      extensions: [CarveKit.configure({ image: false }), CarveImage],
      content: { type: 'doc', content: [{ type: 'paragraph' }] },
      editorProps: {
        handlePaste: (_view, event) => {
          const rejected = this.pasteSanitizer.takePasteRejection();
          return rejected || (this.pasteImage(event) ?? false);
        },
        handleDrop: (_view, event) => this.dropImages(event),
        handleDOMEvents: {
          copy: (_view, event) => this.copySelection(event),
          paste: (view, event) => this.pasteText(view, event),
        },
        transformPasted: (slice, view, plain) =>
          this.pasteSanitizer.resolvePastedSlice(slice, plain, (source) => {
            const result = carveToProseMirrorWithReport(source, {
              unsupported: 'preserve',
            });
            if (unsupportedForPasting(result).length) return null;
            const document = view.state.schema.nodeFromJSON(result.doc);
            return Slice.maxOpen(document.content, true);
          }),
      },
      onUpdate: ({ editor }) => this.onUpdate(editor),
      onSelectionUpdate: () => this.reportSelection(),
      onTransaction: () => this.reportSelection(),
    });
    this.root.addEventListener(
      'paste',
      (event) => {
        if (this.pasteImage(event)) event.stopPropagation();
      },
      true,
    );
    this.root.addEventListener(
      'drop',
      (event) => {
        if (this.dropImages(event)) event.stopPropagation();
      },
      true,
    );
    this.root.addEventListener('pointerdown', (event) => {
      focusEmptyEditorSurface(event, this.editor, this.root);
    });
    this.send({ type: 'ready' });
  }

  public load(source: string, session: number): boolean {
    this.session = session;
    this.revision = 0;
    this.navigationEpoch = 0;
    this.pendingPastes.length = 0;
    const result = carveToProseMirrorWithReport(source, {
      unsupported: 'preserve',
    });
    const unsupported = unsupportedForEditing(result);
    if (unsupported.length) {
      this.send({ type: 'unsupported', session, unsupported, degraded: [] });
      return false;
    }
    this.loading = true;
    try {
      this.currentEditor().commands.setContent(result.doc);
    } finally {
      this.loading = false;
    }
    this.reportSelection();
    return true;
  }

  /** Focuses the current document's native editable surface. */
  public focus(): boolean {
    const editor = this.editor;
    if (!editor) return false;
    editor.view.dom.focus({ preventScroll: true });
    return document.activeElement === editor.view.dom;
  }

  public command(name: string, argument?: unknown): boolean {
    const editor = this.currentEditor();
    const chain = editor.chain().focus();
    switch (name) {
      case 'bold':
        return chain.toggleBold().run();
      case 'italic':
        return chain.toggleItalic().run();
      case 'strike':
        return chain.toggleStrike().run();
      case 'underline':
        return chain.toggleUnderline().run();
      case 'highlight':
        return chain.toggleHighlight().run();
      case 'superscript':
        return chain.toggleSuperscript().run();
      case 'subscript':
        return chain.toggleSubscript().run();
      case 'inline-code':
        return chain.toggleCode().run();
      case 'bullet-list':
        return chain.toggleBulletList().run();
      case 'ordered-list': {
        const removing = editor.isActive('orderedList');
        const ordered = chain.toggleOrderedList();
        return removing
          ? ordered.run()
          : ordered
              .updateAttributes('orderedList', {
                carveBareMarker: true,
                carveDelim: '.',
              })
              .run();
      }
      case 'task-list':
        return chain.toggleTaskList().run();
      case 'code-block':
        return chain.toggleCodeBlock().run();
      case 'heading':
        return typeof argument === 'number' && argument > 0
          ? chain.toggleHeading({ level: argument }).run()
          : chain.setParagraph().run();
      case 'insert-table':
        return this.insertOrResizeTable(argument as TableCommand | undefined);
      case 'image-width':
        return this.setImageWidth(argument);
      case 'insert-link':
        return this.insertLink(argument as LinkCommand | undefined);
      case 'undo':
        return chain.undo().run();
      case 'redo':
        return chain.redo().run();
      default:
        return false;
    }
  }

  /** Selects an authored media occurrence without changing document content. */
  public focusMedia(path: string, occurrence = 0): boolean {
    const editor = this.editor;
    if (!editor) return false;
    const target = mediaOccurrences(editor.state.doc).find(
      (item) => item.path === path && item.occurrence === occurrence,
    );
    if (!target) return false;
    const { from: position, to, image } = target;
    const size = to - position;
    const chain = editor.chain().focus();
    return (
      image
        ? chain.setNodeSelection(position)
        : chain.setTextSelection({ from: position, to: position + size })
    )
      .scrollIntoView()
      .run();
  }

  /** Executes navigation only against the projection version admitted by the host. */
  public focusDocumentTarget(
    target: DocumentTarget,
    session: number,
    revision: number,
    navigationEpoch = this.navigationEpoch,
  ): boolean {
    const editor = this.editor;
    if (!editor || session !== this.session) return false;
    this.navigationEpoch = navigationEpoch;
    if (revision !== this.revision) {
      this.reportSelection();
      return false;
    }
    this.navigating = true;
    try {
      if (target.kind === 'media')
        return this.focusMedia(target.path, target.occurrence);
      const heading = headingOccurrences(editor.state.doc)[target.occurrence];
      return heading
        ? editor
            .chain()
            .setTextSelection(heading.from)
            .focus()
            .scrollIntoView()
            .run()
        : false;
    } finally {
      this.navigating = false;
      this.reportSelection();
    }
  }

  public source(): string {
    const editor = this.editor;
    return editor ? serializeToCarve(editor.getJSON()) : '';
  }

  public linkContext(): LinkContext {
    const editor = this.editor;
    return editor ? linkContext(editor.state) : { text: '', destination: '' };
  }

  public insertImage(path: string, alt = 'Pasted image'): void {
    const editor = this.currentEditor();
    if (this.replacePendingBlobImage(path, alt)) return;
    editor
      .chain()
      .focus()
      .insertContent([
        {
          type: 'paragraph',
          content: [{ type: 'image', attrs: { src: path, alt } }],
        },
        { type: 'paragraph' },
      ])
      .run();
  }

  /** Inserts host-imported pasted text for the request that captured it. */
  public insertPastedSource(
    requestId: number,
    source: string,
    structured: boolean,
    fallbackText: string,
    hostInitiated = false,
  ): void {
    const editor = this.editor;
    if (!editor) return;
    let range: { from: number; to: number };
    if (hostInitiated) {
      range = {
        from: editor.state.selection.from,
        to: editor.state.selection.to,
      };
    } else {
      const index = this.pendingPastes.findIndex(
        (entry) => entry.requestId === requestId,
      );
      if (index < 0) return;
      const [pending] = this.pendingPastes.splice(index, 1);
      if (
        pending.session !== this.session ||
        pending.epoch !== this.userEditEpoch
      ) {
        return;
      }
      range = { from: pending.from, to: pending.to };
    }
    const slice = structured ? this.carveSlice(source, editor) : null;
    this.insertPasteSlice(
      editor,
      range,
      slice ?? plainTextSlice(editor.state.schema, fallbackText),
    );
  }

  private carveSlice(source: string, editor: RuntimeEditor): Slice | null {
    const result = carveToProseMirrorWithReport(source, {
      unsupported: 'preserve',
    });
    if (unsupportedForPasting(result).length) return null;
    const document = editor.state.schema.nodeFromJSON(result.doc);
    return Slice.maxOpen(document.content, true);
  }

  private insertPasteSlice(
    editor: RuntimeEditor,
    range: { from: number; to: number },
    slice: Slice,
  ): void {
    const { state } = editor;
    if (range.to > state.doc.content.size) return;
    this.applyingPaste = true;
    try {
      editor.view.dispatch(
        state.tr
          .replaceRange(range.from, range.to, slice)
          .scrollIntoView()
          .setMeta('paste', true)
          .setMeta('uiEvent', 'paste'),
      );
    } finally {
      this.applyingPaste = false;
    }
    // Keep the remaining paste targets aligned with the insert we just applied.
    const delta = slice.content.size - (range.to - range.from);
    if (delta !== 0) {
      for (const pending of this.pendingPastes) {
        if (pending.from >= range.to) {
          pending.from += delta;
          pending.to += delta;
        }
      }
    }
    editor.view.focus();
  }

  public setTheme(
    dark: boolean,
    accent: string,
    selectionBackground: string,
    selectionForeground: string,
  ): void {
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
    this.themeStyles = `:root { --accent-color: ${accent}; --selection-background: ${selectionBackground}; --selection-foreground: ${selectionForeground}; }`;
    this.applyRuntimeStyles();
  }

  public setAppearance(css: string): void {
    this.appearanceStyles = `:root { ${css} }`;
    this.applyRuntimeStyles();
  }

  private applyRuntimeStyles(): void {
    const runtimeStyles = document.getElementById(RUNTIME_STYLES_ID);
    if (runtimeStyles?.tagName === 'STYLE') {
      runtimeStyles.textContent = `${this.themeStyles}\n${this.appearanceStyles}`;
    }
  }

  private currentEditor(): RuntimeEditor {
    if (!this.editor)
      throw new Error('EditorController must be initialized before use.');
    return this.editor;
  }

  private send(event: EditorEvent): void {
    this.host?.postMessage(JSON.stringify(event));
  }

  private onUpdate(editor: RuntimeEditor): void {
    if (this.loading || this.persistUnexpectedBlobImages()) return;
    // Our own paste insert advances the revision without invalidating the
    // concurrent paste targets captured before it.
    if (!this.applyingPaste) this.userEditEpoch += 1;
    this.revision += 1;
    this.send({
      type: 'changed',
      session: this.session,
      revision: this.revision,
      source: serializeToCarve(editor.getJSON()),
    });
    this.reportSelection();
  }

  private reportSelection(): void {
    const editor = this.editor;
    if (!editor || this.loading || this.navigating) return;
    const active = (name: string, attrs?: Record<string, unknown>) =>
      editor.isActive(name, attrs);
    const heading = editor.getAttributes('heading');
    const image = active('image');
    const states = [
      ['bold', active('bold')],
      ['italic', active('italic')],
      ['strike', active('strike')],
      ['underline', active('underline')],
      ['inline-code', active('code')],
      ['highlight', active('highlight')],
      ['superscript', active('superscript')],
      ['subscript', active('subscript')],
      ['link', active('link')],
      ['bullet-list', active('bulletList')],
      ['ordered-list', active('orderedList')],
      ['task-list', active('taskList')],
      ['code-block', active('codeBlock')],
      ['table', active('table')],
      ['image', image],
    ];
    this.send({
      type: 'selection',
      session: this.session,
      state: {
        active: states.filter(([, enabled]) => enabled).map(([name]) => name),
        heading: active('heading') ? heading.level : 0,
        heading_occurrence: selectedHeading(editor.state),
        revision: this.revision,
        navigation_epoch: this.navigationEpoch,
        image_width: image ? this.imageWidth() : null,
        media: selectedMedia(editor.state),
      },
    });
  }

  private imageWidth(): number {
    const value =
      this.currentEditor().getAttributes('image').carveKeyValues?.width;
    const match = typeof value === 'string' && /^(\d{1,3})%$/.exec(value);
    return match ? Number(match[1]) : 0;
  }

  private insertOrResizeTable(argument?: TableCommand): boolean {
    const editor = this.currentEditor();
    editor.commands.focus();
    const { rows, columns } = tableSize(argument?.rows, argument?.columns);
    const header = argument?.header ?? true;
    if (!editor.isActive('table')) {
      return editor
        .chain()
        .focus()
        .insertTable({ rows, cols: columns, withHeaderRow: header })
        .run();
    }
    const transactions = resizeSelectedTable(editor.state, {
      rows,
      columns,
      header,
    });
    if (!transactions.length) return false;
    transactions.forEach((transaction) => {
      editor.view.dispatch(transaction.scrollIntoView());
    });
    return true;
  }

  private insertLink(argument?: LinkCommand): boolean {
    const editor = this.currentEditor();
    editor.commands.focus();
    const transaction = insertOrUpdateLink(editor.state, argument);
    if (!transaction) return false;
    editor.view.dispatch(transaction);
    return true;
  }

  private setImageWidth(width: unknown): boolean {
    const editor = this.currentEditor();
    const transaction = resizeSelectedImage(editor.state, width);
    if (!transaction) return false;
    editor.view.dispatch(transaction);
    editor.commands.focus();
    return true;
  }

  private copySelection(event: ClipboardEvent): boolean {
    const editor = this.editor;
    if (!editor) return false;
    const source = selectedCarveSource(editor.state);
    if (source == null) return false;
    event.preventDefault();
    this.send({ type: 'copy-selection', session: this.session, source });
    return true;
  }

  private pasteText(view: RuntimeEditor, event: ClipboardEvent): boolean {
    const clipboard = event.clipboardData;
    if (!clipboard) return false;
    const types = [...clipboard.types];
    if (types.includes(CARVER_CLIPBOARD_TYPE)) {
      this.pasteSanitizer.capturePastedSource(
        clipboard.getData(CARVER_CLIPBOARD_TYPE),
      );
      return false;
    }
    this.pasteSanitizer.capturePastedSource(null);
    if (types.some((type) => type.startsWith('image/'))) return false;
    // Rich clipboard content keeps the browser's native paste behavior; smart
    // paste only applies to plain text, where no formatting can be preserved.
    if (types.includes('text/html')) return false;
    const text = clipboard.getData('text/plain');
    if (!text || !mightBeMarkupText(text)) return false;
    event.preventDefault();
    this.nextPasteId += 1;
    this.pendingPastes.push({
      requestId: this.nextPasteId,
      session: this.session,
      epoch: this.userEditEpoch,
      from: view.state.selection.from,
      to: view.state.selection.to,
    });
    this.send({
      type: 'paste-text',
      session: this.session,
      request_id: this.nextPasteId,
      text,
    });
    return true;
  }

  private pasteImage(event: ClipboardEvent): boolean {
    const item = [...(event.clipboardData?.items ?? [])].find((item) =>
      item.type.startsWith('image/'),
    );
    if (!item) return false;
    event.preventDefault();
    const file = item.getAsFile();
    return file ? this.persistImageFile(file) : false;
  }

  private persistImageFile(file: File): boolean {
    if (!file.type.startsWith('image/')) return false;
    const reader = new FileReader();
    reader.onload = () =>
      this.send({
        type: 'paste-image',
        session: this.session,
        mime_type: file.type,
        data: String(reader.result).split(',', 2)[1] ?? '',
      });
    reader.readAsDataURL(file);
    return true;
  }

  private dropImages(event: DragEvent): boolean {
    const files = [...(event.dataTransfer?.files ?? [])].filter((file) =>
      file.type.startsWith('image/'),
    );
    if (!files.length) {
      const uriList = event.dataTransfer?.getData('text/uri-list') ?? '';
      if (
        !/^file:\/\/\/.+\.(?:avif|gif|jpe?g|png|svg|webp)(?:\r?$)/im.test(
          uriList,
        )
      )
        return false;
      event.preventDefault();
      return true;
    }
    event.preventDefault();
    const editor = this.currentEditor();
    const position = editor.view.posAtCoords({
      left: event.clientX,
      top: event.clientY,
    })?.pos;
    if (position != null)
      editor.chain().focus().setTextSelection(position).run();
    files.forEach((file) => {
      this.persistImageFile(file);
    });
    return true;
  }

  private replacePendingBlobImage(path: string, alt: string): boolean {
    const editor = this.currentEditor();
    let pending:
      | { position: number; attrs: Record<string, unknown> }
      | undefined;
    editor.state.doc.descendants((node, position) => {
      if (
        node.type.name === 'image' &&
        typeof node.attrs.src === 'string' &&
        node.attrs.src.startsWith('blob:') &&
        this.pendingBlobSources.has(node.attrs.src)
      ) {
        pending = { position, attrs: node.attrs };
        return false;
      }
      return true;
    });
    if (!pending) return false;
    this.pendingBlobSources.delete(String(pending.attrs.src));
    return editor
      .chain()
      .setNodeSelection(pending.position)
      .updateAttributes('image', {
        ...pending.attrs,
        src: path,
        alt: pending.attrs.alt || alt,
      })
      .run();
  }

  private persistUnexpectedBlobImages(): boolean {
    let found = false;
    this.currentEditor().state.doc.descendants((node) => {
      const source = node.type.name === 'image' ? node.attrs.src : null;
      if (typeof source !== 'string' || !source.startsWith('blob:'))
        return true;
      found = true;
      this.persistBlobImage(source);
      return true;
    });
    return found;
  }

  private persistBlobImage(source: string): void {
    if (this.pendingBlobSources.has(source)) return;
    this.pendingBlobSources.add(source);
    fetch(source)
      .then((response) =>
        response.ok
          ? response.blob()
          : Promise.reject(new Error('blob unavailable')),
      )
      .then(
        (blob) =>
          new Promise<{ data: string; mimeType: string }>((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = () =>
              resolve({
                data: String(reader.result).split(',', 2)[1] ?? '',
                mimeType: blob.type || 'image/png',
              });
            reader.onerror = reject;
            reader.readAsDataURL(blob);
          }),
      )
      .then(({ data, mimeType }) =>
        this.send({
          type: 'paste-image',
          session: this.session,
          mime_type: mimeType,
          data,
        }),
      )
      .catch(() => this.pendingBlobSources.delete(source));
  }
}
