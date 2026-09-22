import { highlightCode } from './editor/code-highlighting';
import { diffLineKind } from './editor/diff-line';

function escapeHtml(value: string): string {
  return value.replace(/[&<>]/g, (character) => {
    if (character === '&') return '&amp;';
    if (character === '<') return '&lt;';
    return '&gt;';
  });
}

function diffLineClass(line: string): string {
  const kind = diffLineKind(line);
  return kind ? ` carver-diff-${kind}` : '';
}

function diffLines(code: HTMLElement): string[] {
  // The native fallback emits one newline-free span per line, so spans are
  // lines and only loose text still needs splitting.
  const lines: string[] = [];
  let text = '';
  for (const node of code.childNodes) {
    if (
      node.nodeType === Node.ELEMENT_NODE &&
      (node as Element).classList.contains('carver-diff-line')
    ) {
      if (text) {
        lines.push(...text.replace(/\n$/, '').split('\n'));
        text = '';
      }
      lines.push(node.textContent ?? '');
    } else {
      text += node.textContent ?? '';
    }
  }
  if (text || lines.length === 0) {
    lines.push(...text.split('\n'));
  }
  return lines;
}

function highlightDiff(code: HTMLElement, language: string | null): void {
  code.innerHTML = diffLines(code)
    .map((line) => {
      const marker = /^[+\- ]/.test(line) ? line[0] : '';
      const content = marker ? line.slice(1) : line;
      const highlighted =
        highlightCode(language, content) ?? escapeHtml(content);
      return `<span class="carver-diff-line${diffLineClass(line)}">${escapeHtml(marker)}${highlighted}</span>`;
    })
    .join('');
}

export function highlightPreviewCode(root: ParentNode = document): void {
  for (const code of root.querySelectorAll<HTMLElement>('pre > code')) {
    const language = [...code.classList]
      .find((className) => className.startsWith('language-'))
      ?.slice('language-'.length);
    if (language && code.parentElement) {
      code.parentElement.dataset.language = language;
    }
    const highlighted = highlightCode(language ?? null, code.textContent ?? '');
    const isDiff = code.parentElement?.classList.contains('diff');
    if (isDiff) {
      highlightDiff(code, language ?? null);
    } else if (highlighted) {
      code.innerHTML = highlighted;
    } else {
      continue;
    }
    code.classList.add('hljs');
  }
}

highlightPreviewCode();
