// @vitest-environment happy-dom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ImageZoom } from '../src/image-zoom';

let viewer: ImageZoom;
let frame: FrameRequestCallback | null;
let notifyResize: ResizeObserverCallback;

function flush() {
  const callback = frame;
  frame = null;
  callback?.(0);
}

function imageFixture(image: HTMLImageElement, x = 100, y = 100) {
  Object.defineProperties(image, {
    complete: { configurable: true, value: true },
    naturalWidth: { configurable: true, value: 400 },
    naturalHeight: { configurable: true, value: 200 },
  });
  vi.spyOn(image, 'getBoundingClientRect').mockReturnValue({
    x,
    y,
    left: x,
    top: y,
    right: x + 200,
    bottom: y + 100,
    width: 200,
    height: 100,
    toJSON: () => ({}),
  });
  return image;
}

function fixture(preview = false) {
  document.body.innerHTML =
    '<main id="editor"><p>Text <img src="carver-asset:///assets/first.png" alt="First" style="width:50%"></p><img src="carver-asset:///assets/second.png" alt="Second"></main>';
  document.body.dataset.imageZoomLabel = 'Zoom image';
  document.body.dataset.imageZoomCloseLabel = 'Close image viewer';
  const images = [...document.querySelectorAll('img')].map((image, index) =>
    imageFixture(image, 100, 100 + index * 200),
  );
  const root = document.querySelector<HTMLElement>('#editor');
  root.tabIndex = 0;
  viewer = new ImageZoom(preview ? document.body : root);
  return {
    root,
    images,
    buttons: [
      ...document.querySelectorAll<HTMLButtonElement>('.image-zoom-button'),
    ],
    dialog: document.querySelector<HTMLDialogElement>('dialog'),
  };
}

beforeEach(() => {
  frame = null;
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
    frame = callback;
    return 1;
  });
  vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  vi.stubGlobal(
    'ResizeObserver',
    class {
      constructor(callback: ResizeObserverCallback) {
        notifyResize = callback;
      }
      observe = vi.fn();
      unobserve = vi.fn();
      disconnect = vi.fn();
    },
  );
});

afterEach(() => {
  viewer?.destroy();
  document.body.replaceChildren();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('image viewing', () => {
  it('ignores editor placeholders without an image source', async () => {
    const { root } = fixture();
    root.insertAdjacentHTML('beforeend', '<img><img src="">');
    await new Promise((resolve) => setTimeout(resolve, 0));
    flush();
    expect(document.querySelectorAll('.image-zoom-button')).toHaveLength(2);
  });
  it('places localized controls outside unchanged source markup', () => {
    const { root, buttons, images } = fixture();
    expect(root.querySelector('button')).toBeNull();
    expect(buttons).toHaveLength(2);
    expect(buttons[0].title).toBe('Zoom image');
    expect(buttons[0].getAttribute('aria-label')).toBe('Zoom image');
    expect(buttons[0].style.left).toBe('264px');
    expect(buttons[0].style.top).toBe('104px');
    expect(images[0].style.width).toBe('50%');
    expect(
      document.querySelector('.image-zoom-close')?.getAttribute('aria-label'),
    ).toBe('Close image viewer');
  });

  it('shows only the hovered image control and retains hover on the button', () => {
    const { images, buttons } = fixture();
    images[0].dispatchEvent(new PointerEvent('pointerover', { bubbles: true }));
    expect(buttons[0].classList.contains('image-zoom-hover')).toBe(true);
    buttons[0].dispatchEvent(
      new PointerEvent('pointerover', { bubbles: true }),
    );
    expect(buttons[0].classList.contains('image-zoom-hover')).toBe(true);
    images[1].dispatchEvent(new PointerEvent('pointerover', { bubbles: true }));
    expect(buttons[0].classList.contains('image-zoom-hover')).toBe(false);
    document.dispatchEvent(new PointerEvent('pointerout'));
    expect(buttons[1].classList.contains('image-zoom-hover')).toBe(false);
    document.dispatchEvent(
      new PointerEvent('pointerout', { relatedTarget: buttons[0] }),
    );
  });

  it('opens the requested image without changing markup or selection and restores focus', () => {
    const { root, buttons, dialog } = fixture();
    const before = root.innerHTML;
    root.focus();
    const range = document.createRange();
    range.selectNodeContents(root.querySelector('p').firstChild);
    window.getSelection()?.addRange(range);
    const selected = window.getSelection()?.toString();
    const down = new PointerEvent('pointerdown', { cancelable: true });
    buttons[1].dispatchEvent(down);
    expect(down.defaultPrevented).toBe(true);
    buttons[1].click();
    expect(dialog.open).toBe(true);
    expect(dialog.querySelector('img')?.alt).toBe('Second');
    expect(dialog.querySelector('img')?.src).toBe(
      'carver-asset:///assets/second.png',
    );
    expect(root.innerHTML).toBe(before);
    document.querySelector<HTMLButtonElement>('.image-zoom-close').click();
    expect(dialog.open).toBe(false);
    expect(document.activeElement).toBe(root);
    expect(window.getSelection()?.toString()).toBe(selected);
    expect(window.scrollTo).toHaveBeenCalledWith(0, 0);
    expect(document.documentElement.classList.contains('image-zoom-open')).toBe(
      false,
    );
  });

  it('allows keyboard focus to return to the image control', () => {
    const { buttons, dialog } = fixture();
    buttons[0].focus();
    buttons[0].click();
    dialog.dispatchEvent(new Event('cancel', { cancelable: true }));
    expect(dialog.open).toBe(false);
    expect(document.activeElement).toBe(buttons[0]);
  });

  it('closes on backdrop but keeps image clicks inert', () => {
    const { buttons, dialog } = fixture();
    buttons[0].click();
    dialog.querySelector('img').click();
    expect(dialog.open).toBe(true);
    dialog.click();
    expect(dialog.open).toBe(false);
    viewer.close();
  });

  it('fits and enlarges the image proportionally and refits after resize', () => {
    const { buttons, dialog } = fixture();
    buttons[0].click();
    const image = dialog.querySelector('img');
    const width = Number.parseFloat(image.style.width);
    expect(width).toBeGreaterThan(400);
    expect(width / Number.parseFloat(image.style.height)).toBe(2);
    expect(width).toBeLessThanOrEqual(window.innerWidth - 64);
    window.dispatchEvent(new Event('resize'));
    notifyResize([], null);
    flush();
    expect(dialog.open).toBe(true);
    buttons[1].click();
    expect(image.alt).toBe('First');
  });

  it('tracks viewport positions on scroll and image loading', () => {
    const { images, buttons } = fixture();
    vi.mocked(images[0].getBoundingClientRect).mockReturnValue(
      new DOMRect(10, -20, 100, 80),
    );
    document.dispatchEvent(new Event('scroll'));
    images[0].dispatchEvent(new Event('load'));
    flush();
    expect(buttons[0].style.left).toBe('74px');
    expect(buttons[0].style.top).toBe('4px');
  });

  it('hides unloaded, failed, zero-size, and offscreen images', () => {
    const { images, buttons } = fixture();
    Object.defineProperty(images[0], 'complete', {
      value: false,
      configurable: true,
    });
    images[0].dispatchEvent(new Event('error'));
    flush();
    expect(buttons[0].hidden).toBe(true);
    buttons[0].click();
    expect(document.querySelector('dialog').open).toBe(false);
    Object.defineProperty(images[0], 'complete', { value: true });
    Object.defineProperty(images[0], 'naturalWidth', { value: 0 });
    images[0].dispatchEvent(new Event('error'));
    flush();
    expect(buttons[0].hidden).toBe(true);
    for (const rect of [
      new DOMRect(0, 0, 0, 0),
      new DOMRect(0, -100, 200, 50),
      new DOMRect(0, window.innerHeight + 1, 200, 50),
      new DOMRect(-300, 0, 200, 50),
      new DOMRect(window.innerWidth + 1, 0, 200, 50),
    ]) {
      vi.mocked(images[1].getBoundingClientRect).mockReturnValue(rect);
      window.dispatchEvent(new Event('resize'));
      flush();
      expect(buttons[1].hidden).toBe(true);
    }
  });

  it('updates controls when document images are added or removed', async () => {
    const { root, images, buttons, dialog } = fixture();
    buttons[0].click();
    images[0].remove();
    const added = imageFixture(document.createElement('img'));
    added.src = 'carver-asset:///assets/added.png';
    root.append(added);
    await new Promise((resolve) => setTimeout(resolve, 0));
    flush();
    expect(buttons[0].isConnected).toBe(false);
    expect(document.querySelectorAll('.image-zoom-button')).toHaveLength(2);
    expect(dialog.open).toBe(false);
    buttons[0].click();
    expect(dialog.open).toBe(false);
  });

  it('closes if the viewed image source changes', async () => {
    const { images, buttons, dialog } = fixture();
    buttons[0].click();
    images[0].src = 'carver-asset:///assets/replaced.png';
    await new Promise((resolve) => setTimeout(resolve, 0));
    flush();
    expect(dialog.open).toBe(false);
  });

  it('ignores its own chrome when observing preview body changes', async () => {
    const { buttons, dialog } = fixture(true);
    buttons[0].click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flush();
    expect(dialog.open).toBe(true);
    expect(document.querySelectorAll('.image-zoom-button')).toHaveLength(2);
  });

  it('dismisses on mode changes and enlarged-image load failure', () => {
    const { buttons, dialog } = fixture();
    buttons[0].click();
    window.dispatchEvent(new Event('carver-dismiss-image-zoom'));
    expect(dialog.open).toBe(false);
    buttons[0].click();
    dialog.querySelector('img').dispatchEvent(new Event('error'));
    expect(dialog.open).toBe(false);
  });

  it('releases observers and controls when the document unloads', () => {
    const { buttons } = fixture();
    buttons[0].click();
    window.dispatchEvent(new Event('pagehide'));
    expect(document.querySelector('dialog')).toBeNull();
    expect(document.querySelector('.image-zoom-controls')).toBeNull();
  });
});
