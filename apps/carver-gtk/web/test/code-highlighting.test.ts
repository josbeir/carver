import { describe, expect, it } from 'vitest';

import { highlightCode } from '../src/editor/code-highlighting';

describe('highlightCode', () => {
  it('highlights JavaScript and Carve code fences', () => {
    expect(highlightCode('js', 'const value = "text";')).toContain(
      'hljs-keyword',
    );
    expect(highlightCode('carve', '# *Heading*')).toContain('hljs');
    expect(highlightCode('crv', '# *Heading*')).toContain('hljs');
  });

  it('leaves an unknown fence language alone', () => {
    expect(highlightCode('unknown-fence', 'plain text')).toBeNull();
  });

  it('highlights headings after a closed Carve code fence longer than 32 KB', () => {
    const source = `\`\`\`text\n${'x'.repeat(33 * 1024)}\n\`\`\`\n\n# After`;

    expect(highlightCode('carve', source)).toContain(
      '<span class="hljs-section"># After</span>',
    );
  });
});
