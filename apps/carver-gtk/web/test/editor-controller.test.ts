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
  const createEditor = vi.fn(() => editor);
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
