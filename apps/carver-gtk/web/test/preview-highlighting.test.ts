// @vitest-environment happy-dom

import { describe, expect, it } from 'vitest';

import { highlightPreviewCode } from '../src/preview-highlighting';

describe('highlightPreviewCode', () => {
  it('keeps diff lines and applies JavaScript token colors inside them', () => {
    document.body.innerHTML =
      '<pre class="diff"><code class="language-js"><span class="carver-diff-line carver-diff-add">+const value = "text";</span></code></pre>';

    highlightPreviewCode(document);

    const line = document.querySelector('.carver-diff-line');
    expect(line?.classList.contains('carver-diff-add')).toBe(true);
    expect(line?.innerHTML).toContain('hljs-keyword');
  });

  it('highlights unchanged context lines in a diff block', () => {
    document.body.innerHTML =
      '<pre class="diff"><code class="language-js">  let fileIcon = document.querySelector("li.file-entry");\n<span class="carver-diff-line carver-diff-add">+ fileIcon.classList.remove("icon-file-text");</span></code></pre>';

    highlightPreviewCode(document);

    expect(document.querySelector('code')?.innerHTML).toContain('hljs-keyword');
    expect(document.querySelectorAll('.carver-diff-line')).toHaveLength(2);
    expect(document.querySelector('code')?.childNodes).toHaveLength(2);
  });

  it('keeps one row per line for the native diff fallback shape', () => {
    document.body.innerHTML =
      '<pre class="diff"><code class="language-js"><span class="carver-diff-line">  let fileIcon = 1;</span><span class="carver-diff-line carver-diff-remove">- fileIcon.add("a");</span><span class="carver-diff-line carver-diff-add">+ fileIcon.remove("a");</span></code></pre>';

    highlightPreviewCode(document);

    const lines = document.querySelectorAll('.carver-diff-line');
    expect(lines).toHaveLength(3);
    expect(lines[0]?.innerHTML).toContain('hljs-keyword');
    expect(lines[1]?.classList.contains('carver-diff-remove')).toBe(true);
    expect(lines[2]?.classList.contains('carver-diff-add')).toBe(true);
  });

  it('keeps diff semantics when the fence language is unknown', () => {
    document.body.innerHTML =
      '<pre class="diff"><code class="language-unknown-fence">+&lt;added&gt;\n-removed\n@@ -1 +1 @@</code></pre>';

    highlightPreviewCode(document);

    expect(document.querySelector('.carver-diff-add')?.textContent).toBe(
      '+<added>',
    );
    expect(document.querySelector('.carver-diff-remove')?.textContent).toBe(
      '-removed',
    );
    expect(document.querySelector('.carver-diff-hunk')?.textContent).toBe(
      '@@ -1 +1 @@',
    );
  });

  it('labels fences with their declared language', () => {
    document.body.innerHTML =
      '<pre><code class="language-crv">## Heading</code></pre><pre><code class="language-unknown-fence">x</code></pre><pre><code>plain</code></pre>';

    highlightPreviewCode(document);

    const blocks = document.querySelectorAll('pre');
    expect(blocks[0]?.dataset.language).toBe('crv');
    expect(blocks[1]?.dataset.language).toBe('unknown-fence');
    expect(blocks[2]?.hasAttribute('data-language')).toBe(false);
  });

  it('leaves ordinary unknown fences unchanged', () => {
    document.body.innerHTML =
      '<pre><code class="language-unknown-fence">plain text</code></pre>';

    highlightPreviewCode(document);

    expect(document.querySelector('code')?.innerHTML).toBe('plain text');
  });

  it('highlights Carve fences', () => {
    document.body.innerHTML =
      '<pre><code class="language-carve"># *Heading*</code></pre>';

    highlightPreviewCode(document);

    expect(document.querySelector('code')?.classList.contains('hljs')).toBe(
      true,
    );
    expect(document.querySelector('code')?.innerHTML).toContain('hljs');
  });
});
