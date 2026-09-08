import { describe, expect, it, vi } from 'vitest';
import { getSchema } from '@tiptap/core';
import { CarveKit } from '@markup-carve/carve-grammars/tiptap';
import type { Slice } from '@tiptap/pm/model';

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
    const restored = options.editorProps.transformPasted(
      { foreign: true },
      { state: { schema: getSchema([CarveKit]) } },
      false,
    );

    expect(restored.size).toBe(0);
  });

  it('routes named commands through its current editor chain', () => {
    const { chain, controller } = controllerFixture();
    controller.initialize();

    expect(controller.command('bold')).toBe(true);
    expect(chain.toggleBold).toHaveBeenCalledOnce();
    expect(controller.command('unknown-command')).toBe(false);
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
