import { describe, expect, it, vi } from 'vitest';

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
    state: { doc: { descendants: vi.fn() } },
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
      clipboardSerializer: expect.objectContaining({
        serializeFragment: expect.any(Function),
      }),
      transformCopied: expect.any(Function),
      transformPasted: expect.any(Function),
      transformPastedHTML: expect.any(Function),
    });
    expect([...listeners.keys()].sort()).toEqual([
      'drop',
      'paste',
      'pointerdown',
    ]);
    expect(messages).toEqual([JSON.stringify({ type: 'ready' })]);
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
