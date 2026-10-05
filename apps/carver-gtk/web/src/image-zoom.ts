/** Viewing chrome lives outside document markup and never enters the editor model. */
export class ImageZoom {
  private readonly controls = new Map<HTMLImageElement, HTMLButtonElement>();
  private readonly layer = document.createElement('div');
  private readonly dialog = document.createElement('dialog');
  private readonly enlarged = document.createElement('img');
  private readonly observer: MutationObserver;
  private readonly resizeObserver: ResizeObserver;
  private readonly listeners = new AbortController();
  private frame = 0;
  private activeImage: HTMLImageElement | null = null;
  private returnFocus: HTMLElement | null = null;
  private selection: Range | null = null;
  private scroll = { x: 0, y: 0 };

  constructor(private readonly root: HTMLElement) {
    const { imageZoomLabel, imageZoomCloseLabel } = document.body.dataset;
    this.layer.className = 'image-zoom-controls';
    this.dialog.className = 'image-zoom-dialog';
    this.dialog.setAttribute('aria-label', imageZoomLabel);
    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'image-zoom-close';
    close.textContent = '×';
    close.title = imageZoomCloseLabel;
    close.setAttribute('aria-label', imageZoomCloseLabel);
    close.autofocus = true;
    this.dialog.append(this.enlarged, close);
    document.body.append(this.layer, this.dialog);
    const options = { signal: this.listeners.signal };
    close.addEventListener('click', () => this.close(), options);
    this.dialog.addEventListener(
      'click',
      (event) => {
        if (event.target === this.dialog) this.close();
      },
      options,
    );
    this.dialog.addEventListener(
      'cancel',
      (event) => {
        event.preventDefault();
        this.close();
      },
      options,
    );
    this.enlarged.addEventListener('error', () => this.close(), options);
    window.addEventListener(
      'carver-dismiss-image-zoom',
      () => this.close(),
      options,
    );
    window.addEventListener('pagehide', () => this.destroy(), options);
    window.addEventListener('resize', () => this.schedule(), options);
    document.addEventListener('scroll', () => this.schedule(), {
      ...options,
      capture: true,
    });
    root.addEventListener('load', () => this.schedule(), {
      ...options,
      capture: true,
    });
    root.addEventListener('error', () => this.schedule(), {
      ...options,
      capture: true,
    });
    document.addEventListener(
      'pointerover',
      (event) => {
        for (const [image, button] of this.controls) {
          button.classList.toggle(
            'image-zoom-hover',
            event.target === image || event.target === button,
          );
        }
      },
      options,
    );
    document.addEventListener(
      'pointerout',
      (event) => {
        if (!event.relatedTarget) {
          for (const button of this.controls.values())
            button.classList.remove('image-zoom-hover');
        }
      },
      options,
    );
    this.observer = new MutationObserver((records) => {
      if (
        records.some(
          ({ target }) =>
            !this.layer.contains(target) && !this.dialog.contains(target),
        )
      ) {
        this.schedule();
      }
    });
    this.observer.observe(root, {
      childList: true,
      subtree: true,
      attributes: true,
    });
    this.resizeObserver = new ResizeObserver(() => this.schedule());
    this.resizeObserver.observe(root);
    this.refresh();
  }

  /** Dismisses viewing state while retaining the source selection and viewport. */
  public close(): void {
    if (!this.dialog.open) return;
    this.dialog.close();
    document.documentElement.classList.remove('image-zoom-open');
    this.enlarged.removeAttribute('src');
    this.activeImage = null;
    const selection = window.getSelection();
    if (
      selection &&
      this.selection?.startContainer.isConnected &&
      this.selection.endContainer.isConnected
    ) {
      selection.removeAllRanges();
      selection.addRange(this.selection);
    }
    // WebKit can focus contenteditable while restoring a DOM range. Restore
    // the invoking control last so keyboard users return to the zoom button.
    if (this.returnFocus?.isConnected)
      this.returnFocus.focus({ preventScroll: true });
    window.scrollTo(this.scroll.x, this.scroll.y);
    this.selection = null;
    this.returnFocus = null;
  }

  /** Releases observers and chrome when the containing document is discarded. */
  public destroy(): void {
    this.close();
    this.listeners.abort();
    this.observer.disconnect();
    this.resizeObserver.disconnect();
    window.cancelAnimationFrame(this.frame);
    this.layer.remove();
    this.dialog.remove();
    this.controls.clear();
  }

  private schedule(): void {
    if (this.frame) return;
    this.frame = window.requestAnimationFrame(() => {
      this.frame = 0;
      this.refresh();
    });
  }

  private refresh(): void {
    const images = new Set(
      [...this.root.querySelectorAll<HTMLImageElement>('img[src]')].filter(
        (image) => image.getAttribute('src').trim() !== '',
      ),
    );
    // Preview's root is the body, which also contains our own viewing chrome.
    images.delete(this.enlarged);
    for (const [image, button] of this.controls) {
      if (!images.has(image)) {
        button.remove();
        this.resizeObserver.unobserve(image);
        this.controls.delete(image);
      }
    }
    if (
      this.activeImage &&
      (!images.has(this.activeImage) ||
        (this.activeImage.currentSrc || this.activeImage.src) !==
          this.enlarged.src)
    )
      this.close();
    this.fit();
    for (const image of images) {
      let button = this.controls.get(image);
      if (!button) {
        button = document.createElement('button');
        button.type = 'button';
        button.className = 'image-zoom-button';
        button.title = document.body.dataset.imageZoomLabel;
        button.setAttribute('aria-label', document.body.dataset.imageZoomLabel);
        // A CSS mask supplies the icon; text content stays out of clipboard output.
        button.addEventListener('pointerdown', (event) =>
          event.preventDefault(),
        );
        button.addEventListener('click', () => this.open(image, button));
        this.controls.set(image, button);
        this.layer.append(button);
        this.resizeObserver.observe(image);
      }
      const rect = image.getBoundingClientRect();
      button.hidden =
        !image.complete ||
        !image.naturalWidth ||
        rect.width <= 0 ||
        rect.height <= 0 ||
        rect.bottom <= 0 ||
        rect.top >= window.innerHeight ||
        rect.right <= 0 ||
        rect.left >= window.innerWidth;
      button.style.left = `${Math.max(rect.left, rect.right - 36)}px`;
      button.style.top = `${Math.max(0, rect.top) + 4}px`;
    }
  }

  private open(image: HTMLImageElement, button: HTMLButtonElement): void {
    if (
      !image.isConnected ||
      !image.complete ||
      !image.naturalWidth ||
      this.dialog.open
    )
      return;
    this.returnFocus =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : button;
    const selection = window.getSelection();
    this.selection = selection?.rangeCount
      ? selection.getRangeAt(0).cloneRange()
      : null;
    this.scroll = { x: window.scrollX, y: window.scrollY };
    this.activeImage = image;
    this.enlarged.src = image.currentSrc || image.src;
    this.enlarged.alt = image.alt;
    this.fit();
    document.documentElement.classList.add('image-zoom-open');
    this.dialog.showModal();
  }

  private fit(): void {
    if (!this.activeImage) return;
    const scale = Math.min(
      Math.max(1, window.innerWidth - 64) / this.activeImage.naturalWidth,
      Math.max(1, window.innerHeight - 152) / this.activeImage.naturalHeight,
    );
    this.enlarged.style.width = `${this.activeImage.naturalWidth * scale}px`;
    this.enlarged.style.height = `${this.activeImage.naturalHeight * scale}px`;
  }
}
