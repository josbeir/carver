import { describe, expect, it, vi } from 'vitest';
import { getSchema } from '@tiptap/core';
import { CarveKit } from '@markup-carve/carve-grammars/tiptap';
import { Fragment, Slice } from '@tiptap/pm/model';
import { EditorState, type Transaction } from '@tiptap/pm/state';

import { EditorController } from '../src/editor/editor-controller';

function controllerFixture() {
  const listeners = new Map<string, EventListener>();
  const root = {
    addEventListener: (name: string, listener: EventListener) =>
      listeners.set(name, listener),
  } as unknown as HTMLElement;
  const messages: string[] = [];
  const run = vi.fn(() => true);
  const chain = {
    focus: vi.fn(() => chain),
    setNodeSelection: vi.fn(() => chain),
    setTextSelection: vi.fn(() => chain),
    scrollIntoView: vi.fn(() => chain),
    toggleBold: vi.fn(() => chain),
    toggleItalic: vi.fn(() => chain),
    toggleStrike: vi.fn(() => chain),
    toggleUnderline: vi.fn(() => chain),
    toggleHighlight: vi.fn(() => chain),
    toggleSuperscript: vi.fn(() => chain),
    toggleSubscript: vi.fn(() => chain),
    toggleCode: vi.fn(() => chain),
    toggleBulletList: vi.fn(() => chain),
    toggleOrderedList: vi.fn(() => chain),
    updateAttributes: vi.fn(() => chain),
    toggleTaskList: vi.fn(() => chain),
    toggleCodeBlock: vi.fn(() => chain),
    toggleHeading: vi.fn(() => chain),
    setParagraph: vi.fn(() => chain),
    undo: vi.fn(() => chain),
    redo: vi.fn(() => chain),
    run,
  };
  const editor = {
    chain: vi.fn(() => chain),
    commands: { setContent: vi.fn(), focus: vi.fn() },
    getAttributes: vi.fn(() => ({})),
    isActive: vi.fn(() => false),
    state: {
      doc: { descendants: vi.fn() },
      selection: { empty: true },
    },
    view: { dispatch: vi.fn() },
  };
  const createEditor = vi.fn((_options: Record<string, unknown>) => editor);
  const controller = new EditorController(
    root,
    { postMessage: (message) => messages.push(message) },
    createEditor,
  );
  return { chain, controller, createEditor, editor, listeners, messages };
}

describe('EditorController', () => {
  it('keeps runtime theme and appearance rules in the head stylesheet', () => {
    const { controller } = controllerFixture();
    const stylesheet = { tagName: 'STYLE', textContent: '' };
    const documentElement = {
      dataset: {},
      getAttribute: (_name: string) => null,
    };
    vi.stubGlobal('document', {
      documentElement,
      getElementById: vi.fn(() => stylesheet),
    });

    controller.setTheme(false, '#358e45', 'rgb(53 142 69 / 25%)', '#333334');
    controller.setAppearance('--document-font-family: "Cantarell";');

    expect(documentElement.getAttribute('style')).toBeNull();
    expect(stylesheet.textContent).toContain('--accent-color: #358e45');
    expect(stylesheet.textContent).toContain(
      '--document-font-family: "Cantarell"',
    );
    vi.unstubAllGlobals();
  });

  it('owns initialization, browser listeners, and the ready event', () => {
    const { controller, createEditor, listeners, messages } =
      controllerFixture();

    controller.initialize();

    expect(createEditor).toHaveBeenCalledOnce();
    expect(createEditor.mock.calls[0][0].editorProps).toMatchObject({
      handleDOMEvents: expect.objectContaining({
        copy: expect.any(Function),
        paste: expect.any(Function),
      }),
      transformPasted: expect.any(Function),
    });
    expect([...listeners.keys()].sort()).toEqual([
      'drop',
      'paste',
      'pointerdown',
    ]);
    expect(messages).toEqual([JSON.stringify({ type: 'ready' })]);
  });

  it('routes a non-empty copy selection through the native clipboard bridge', () => {
    const { controller, createEditor, editor, messages } = controllerFixture();
    const selectedSlice = {
      content: {
        forEach: (visit) =>
          visit({
            isInline: false,
            toJSON: () => ({
              type: 'paragraph',
              content: [{ type: 'text', text: 'Selected' }],
            }),
          }),
      },
    };
    Object.assign(editor.state, {
      selection: {
        empty: false,
        content: () => selectedSlice,
      },
    });
    controller.initialize();
    controller.load('Document', 9);
    const options = createEditor.mock.calls[0]?.[0] as {
      editorProps: {
        handleDOMEvents: {
          copy: (view: unknown, event: ClipboardEvent) => boolean;
          paste: (view: unknown, event: ClipboardEvent) => boolean;
        };
        transformPasted: (
          slice: unknown,
          view: unknown,
          plain: boolean,
        ) => unknown;
      };
    };
    const event = { preventDefault: vi.fn() } as unknown as ClipboardEvent;

    expect(options.editorProps.handleDOMEvents.copy(null, event)).toBe(true);
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(messages).toContain(
      JSON.stringify({
        type: 'copy-selection',
        session: 9,
        source: 'Selected',
      }),
    );
    const pasteEvent = {
      clipboardData: {
        types: ['text/html', 'application/x-carver-source'],
        getData: (type: string) =>
          type === 'application/x-carver-source' ? 'Selected' : '',
      },
    } as unknown as ClipboardEvent;
    expect(options.editorProps.handleDOMEvents.paste(null, pasteEvent)).toBe(
      false,
    );
    const restored = options.editorProps.transformPasted(
      { foreign: true },
      { state: { schema: getSchema([CarveKit]) } },
      false,
    ) as Slice;
    expect(restored.content.textBetween(0, restored.content.size)).toBe(
      'Selected',
    );
  });

  it('refuses a Carver paste that depends on document-level source metadata', () => {
    const { controller, createEditor } = controllerFixture();
    controller.initialize();
    const options = createEditor.mock.calls[0]?.[0] as {
      editorProps: {
        handlePaste: (
          view: unknown,
          event: ClipboardEvent,
          slice: Slice,
        ) => boolean;
        handleDOMEvents: {
          paste: (view: unknown, event: ClipboardEvent) => boolean;
        };
        transformPasted: (
          slice: unknown,
          view: unknown,
          plain: boolean,
        ) => Slice;
      };
    };
    const source =
      '![First](assets/first.png){width="50%"}![Second](assets/second.png)';
    const pasteEvent = {
      clipboardData: {
        types: ['application/x-carver-source'],
        getData: () => source,
      },
    } as unknown as ClipboardEvent;

    options.editorProps.handleDOMEvents.paste(null, pasteEvent);
    const schema = getSchema([CarveKit]);
    const parsed = new Slice(
      Fragment.from(
        schema.nodes.paragraph.create(null, schema.text('Rendered fallback')),
      ),
      0,
      0,
    );
    const restored = options.editorProps.transformPasted(
      parsed,
      { state: { schema } },
      false,
    );

    expect(restored).toBe(parsed);
    expect(options.editorProps.handlePaste(null, pasteEvent, restored)).toBe(
      true,
    );
  });

  it('routes named commands through its current editor chain', () => {
    const { chain, controller } = controllerFixture();
    controller.initialize();

    expect(controller.command('bold')).toBe(true);
    expect(chain.toggleBold).toHaveBeenCalledOnce();
    expect(controller.command('unknown-command')).toBe(false);
  });

  it('creates ordered lists with Carve bare-dot markers', () => {
    const { chain, controller } = controllerFixture();
    controller.initialize();

    expect(controller.command('ordered-list')).toBe(true);
    expect(chain.toggleOrderedList).toHaveBeenCalledOnce();
    expect(chain.updateAttributes).toHaveBeenCalledWith('orderedList', {
      carveBareMarker: true,
      carveDelim: '.',
    });
  });

  it('removes an active ordered list without rewriting marker attributes', () => {
    const { chain, controller, editor } = controllerFixture();
    editor.isActive.mockImplementation(
      (name?: string) => name === 'orderedList',
    );
    controller.initialize();

    expect(controller.command('ordered-list')).toBe(true);
    expect(chain.toggleOrderedList).toHaveBeenCalledOnce();
    expect(chain.updateAttributes).not.toHaveBeenCalled();
  });

  it('returns a heading to normal text when given level zero', () => {
    const { chain, controller } = controllerFixture();
    controller.initialize();

    expect(controller.command('heading', 0)).toBe(true);
    expect(chain.setParagraph).toHaveBeenCalledOnce();
    expect(chain.toggleHeading).not.toHaveBeenCalled();
  });

  it('has safe empty public read results before initialization', () => {
    const { controller } = controllerFixture();

    expect(controller.source()).toBe('');
    expect(controller.linkContext()).toEqual({ text: '', destination: '' });
  });
});

describe('media focus', () => {
  it('selects the requested duplicate image without changing content', () => {
    const { controller, editor, chain } = controllerFixture();
    controller.initialize();
    editor.state.doc.descendants.mockImplementation((visit) => {
      const image = {
        type: { name: 'image' },
        attrs: { src: 'assets/a.png' },
        nodeSize: 1,
      };
      visit(image, 2);
      visit(image, 8);
      visit(image, 12);
    });
    expect(controller.focusMedia('assets/a.png', 1)).toBe(true);
    expect(chain.setNodeSelection).toHaveBeenCalledWith(8);
    expect(chain.scrollIntoView).toHaveBeenCalledOnce();
    expect(editor.commands.setContent).not.toHaveBeenCalled();
  });

  it('selects an attachment link and returns focus to the editor', () => {
    const { controller, editor, chain } = controllerFixture();
    controller.initialize();
    editor.state.doc.descendants.mockImplementation((visit) => {
      visit(
        {
          type: { name: 'text' },
          isText: true,
          nodeSize: 6,
          marks: [{ type: { name: 'link' }, attrs: { href: 'assets/a.pdf' } }],
        },
        4,
      );
    });
    expect(controller.focusMedia('assets/a.pdf')).toBe(true);
    expect(chain.setTextSelection).toHaveBeenCalledWith({ from: 4, to: 10 });
    expect(chain.focus).toHaveBeenCalledOnce();
  });

  it('leaves selection unchanged for unloaded or missing media', () => {
    const { controller, chain } = controllerFixture();
    expect(controller.focusMedia('assets/missing.png')).toBe(false);
    controller.initialize();
    expect(controller.focusMedia('assets/missing.png')).toBe(false);
    expect(chain.focus).not.toHaveBeenCalled();
  });
});

describe('document navigation', () => {
  it('focuses a duplicate heading using its projection position without editing', () => {
    const { controller, editor, chain } = controllerFixture();
    controller.initialize();
    editor.state.doc.descendants.mockImplementation((visit) => {
      visit({ type: { name: 'heading' }, nodeSize: 8 }, 0);
      visit({ type: { name: 'heading' }, nodeSize: 8 }, 12);
    });
    expect(
      controller.focusDocumentTarget({ kind: 'heading', occurrence: 1 }, 0, 0),
    ).toBe(true);
    expect(chain.setTextSelection).toHaveBeenCalledWith(13);
    expect(editor.commands.setContent).not.toHaveBeenCalled();
  });

  it('ignores missing headings and stale projection sessions or revisions', () => {
    const { controller, chain } = controllerFixture();
    const target = { kind: 'heading', occurrence: 0 } as const;
    expect(controller.focusDocumentTarget(target, 0, 0)).toBe(false);
    controller.initialize();
    expect(controller.focusDocumentTarget(target, 1, 0)).toBe(false);
    expect(controller.focusDocumentTarget(target, 0, 1)).toBe(false);
    expect(controller.focusDocumentTarget(target, 0, 0)).toBe(false);
    expect(chain.focus).not.toHaveBeenCalled();
  });

  it('routes media through the same version-checked navigation entry point', () => {
    const { controller, editor, chain } = controllerFixture();
    controller.initialize();
    editor.state.doc.descendants.mockImplementation((visit) => {
      visit(
        {
          type: { name: 'image' },
          attrs: { src: 'assets/a.png' },
          nodeSize: 1,
        },
        2,
      );
    });
    expect(
      controller.focusDocumentTarget(
        { kind: 'media', path: 'assets/a.png', occurrence: 0 },
        0,
        0,
      ),
    ).toBe(true);
    expect(chain.setNodeSelection).toHaveBeenCalledWith(2);
  });
});

describe('smart paste', () => {
  function pasteOptions(createEditor: ReturnType<typeof vi.fn>) {
    return createEditor.mock.calls[0]?.[0] as {
      editorProps: {
        handleDOMEvents: {
          paste: (view: unknown, event: ClipboardEvent) => boolean;
        };
      };
    };
  }

  it('forwards markup-suspect text to the native host', () => {
    const { controller, createEditor, messages } = controllerFixture();
    controller.initialize();
    controller.load('Document', 4);
    const event = {
      preventDefault: vi.fn(),
      clipboardData: {
        types: ['text/plain'],
        getData: () => '**bold**',
      },
    } as unknown as ClipboardEvent;
    const view = { state: { selection: { from: 2, to: 5 } } };

    expect(
      pasteOptions(createEditor).editorProps.handleDOMEvents.paste(view, event),
    ).toBe(true);
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(messages).toContain(
      JSON.stringify({
        type: 'paste-text',
        session: 4,
        request_id: 1,
        text: '**bold**',
      }),
    );
  });

  it('leaves ordinary prose to the native paste path', () => {
    const { controller, createEditor, messages } = controllerFixture();
    controller.initialize();
    controller.load('Document', 4);
    const event = {
      preventDefault: vi.fn(),
      clipboardData: {
        types: ['text/plain'],
        getData: () => 'An ordinary sentence.',
      },
    } as unknown as ClipboardEvent;

    expect(
      pasteOptions(createEditor).editorProps.handleDOMEvents.paste(
        { state: { selection: { from: 0, to: 0 } } },
        event,
      ),
    ).toBe(false);
    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(messages.some((message) => message.includes('paste-text'))).toBe(
      false,
    );
  });

  it('leaves rich clipboard content to the native paste path', () => {
    const { controller, createEditor } = controllerFixture();
    controller.initialize();
    controller.load('Document', 4);
    const event = {
      preventDefault: vi.fn(),
      clipboardData: {
        types: ['text/html', 'text/plain'],
        getData: () => '**bold**',
      },
    } as unknown as ClipboardEvent;

    expect(
      pasteOptions(createEditor).editorProps.handleDOMEvents.paste(
        { state: { selection: { from: 0, to: 0 } } },
        event,
      ),
    ).toBe(false);
    expect(event.preventDefault).not.toHaveBeenCalled();
  });

  it('inserts imported structured Carve into the captured selection', () => {
    const { controller, createEditor, editor } = controllerFixture();
    controller.initialize();
    controller.load('Document', 1);
    const event = {
      preventDefault: vi.fn(),
      clipboardData: {
        types: ['text/plain'],
        getData: () => '# Heading',
      },
    } as unknown as ClipboardEvent;
    pasteOptions(createEditor).editorProps.handleDOMEvents.paste(
      { state: { selection: { from: 0, to: 0 } } },
      event,
    );

    const schema = getSchema([CarveKit]);
    const doc = schema.nodeFromJSON({
      type: 'doc',
      content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Hi' }] }],
    });
    editor.state = EditorState.create({
      doc,
    }) as unknown as typeof editor.state;
    const dispatch = vi.fn();
    editor.view = { dispatch, focus: vi.fn() } as unknown as typeof editor.view;

    controller.insertPastedSource(1, '# Heading\n', true, '# Heading');

    expect(dispatch).toHaveBeenCalledOnce();
    const transaction = dispatch.mock.calls[0][0];
    expect(transaction.doc.textContent).toContain('Heading');
  });

  it('inserts unstructured text faithfully when the host says it is plain', () => {
    const { controller, createEditor, editor } = controllerFixture();
    controller.initialize();
    controller.load('Document', 1);
    const event = {
      preventDefault: vi.fn(),
      clipboardData: {
        types: ['text/plain'],
        getData: () => 'a = b',
      },
    } as unknown as ClipboardEvent;
    pasteOptions(createEditor).editorProps.handleDOMEvents.paste(
      { state: { selection: { from: 0, to: 0 } } },
      event,
    );

    const schema = getSchema([CarveKit]);
    const doc = schema.nodeFromJSON({
      type: 'doc',
      content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Hi' }] }],
    });
    editor.state = EditorState.create({
      doc,
    }) as unknown as typeof editor.state;
    const dispatch = vi.fn();
    editor.view = { dispatch, focus: vi.fn() } as unknown as typeof editor.view;

    controller.insertPastedSource(1, 'a = b', false, 'a = b');

    expect(dispatch).toHaveBeenCalledOnce();
    const transaction = dispatch.mock.calls[0][0];
    expect(transaction.doc.textContent).toContain('a = b');
  });

  it('drops an import that resolves after the document changes', () => {
    const { controller, createEditor, editor } = controllerFixture();
    controller.initialize();
    controller.load('Document', 1);
    const event = {
      preventDefault: vi.fn(),
      clipboardData: {
        types: ['text/plain'],
        getData: () => '# Heading',
      },
    } as unknown as ClipboardEvent;
    pasteOptions(createEditor).editorProps.handleDOMEvents.paste(
      { state: { selection: { from: 0, to: 0 } } },
      event,
    );
    const schema = getSchema([CarveKit]);
    const doc = schema.nodeFromJSON({
      type: 'doc',
      content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Hi' }] }],
    });
    editor.state = EditorState.create({
      doc,
    }) as unknown as typeof editor.state;
    const dispatch = vi.fn();
    editor.view = { dispatch, focus: vi.fn() } as unknown as typeof editor.view;

    controller.load('Replacement', 2);
    controller.insertPastedSource(1, '# Heading\n', true, '# Heading');

    expect(dispatch).not.toHaveBeenCalled();
  });

  it('applies concurrent paste replies without losing either one', () => {
    const { controller, createEditor, editor } = controllerFixture();
    controller.initialize();
    controller.load('Document', 1);
    const paste = pasteOptions(createEditor).editorProps.handleDOMEvents.paste;
    const request = (from: number, to: number) => {
      const event = {
        preventDefault: vi.fn(),
        clipboardData: {
          types: ['text/plain'],
          getData: () => '# Heading',
        },
      } as unknown as ClipboardEvent;
      paste({ state: { selection: { from, to } } }, event);
    };
    request(0, 0);
    request(1, 1);

    const schema = getSchema([CarveKit]);
    const doc = schema.nodeFromJSON({
      type: 'doc',
      content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Hi' }] }],
    });
    editor.state = EditorState.create({
      doc,
    }) as unknown as typeof editor.state;
    const dispatch = vi.fn((transaction: Transaction) => {
      const state = editor.state as unknown as EditorState;
      editor.state = state.apply(transaction) as unknown as typeof editor.state;
    });
    editor.view = { dispatch, focus: vi.fn() } as unknown as typeof editor.view;

    controller.insertPastedSource(1, '# One\n', true, '# One');
    controller.insertPastedSource(2, '# Two\n', true, '# Two');

    expect(dispatch).toHaveBeenCalledTimes(2);
  });
});
