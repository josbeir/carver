import { CarveFrontmatter } from '@markup-carve/carve-grammars/tiptap/extensions/carve-frontmatter.js';
import {
  frontmatterFieldValue,
  frontmatterFieldsAreEditable,
  setFrontmatterField,
} from './frontmatter-fields';

const FIELDS = ['title', 'lang', 'author', 'description'] as const;
let nextBodyId = 0;

/** Carver's frontmatter editor with structured JSON field updates. */
export const CarverFrontmatter = CarveFrontmatter.extend({
  addNodeView() {
    return ({ node, editor, getPos }) => {
      let current = node;
      const dom = document.createElement('section');
      dom.className = 'carve-frontmatter-card';
      dom.contentEditable = 'false';

      const toggle = document.createElement('button');
      toggle.type = 'button';
      toggle.className = 'carve-frontmatter-summary';
      toggle.setAttribute('aria-expanded', 'false');

      const body = document.createElement('div');
      body.className = 'carve-frontmatter-body';
      body.id = `carve-frontmatter-body-${++nextBodyId}`;
      body.hidden = true;
      toggle.setAttribute('aria-controls', body.id);

      const fields = document.createElement('div');
      fields.className = 'carve-frontmatter-fields';
      const inputs = new Map<string, HTMLInputElement | HTMLTextAreaElement>();
      for (const key of FIELDS) {
        const label = document.createElement('label');
        label.textContent = key[0].toUpperCase() + key.slice(1);
        const input =
          key === 'description'
            ? document.createElement('textarea')
            : document.createElement('input');
        input.name = key;
        input.autocomplete = 'off';
        label.appendChild(input);
        fields.appendChild(label);
        inputs.set(key, input);
      }

      const rawLabel = document.createElement('label');
      rawLabel.className = 'carve-frontmatter-raw';
      const rawLabelText = document.createTextNode('');
      const raw = document.createElement('textarea');
      raw.spellcheck = false;
      rawLabel.appendChild(rawLabelText);
      rawLabel.appendChild(raw);
      body.appendChild(fields);
      body.appendChild(rawLabel);
      dom.appendChild(toggle);
      dom.appendChild(body);

      const commit = (content: string) => {
        if (
          !editor.isEditable ||
          typeof getPos !== 'function' ||
          content === current.attrs.content
        ) {
          return;
        }
        editor
          .chain()
          .command(({ tr }) => {
            tr.setNodeMarkup(getPos(), undefined, {
              ...current.attrs,
              content,
            });
            return true;
          })
          .run();
      };
      const render = () => {
        const format = current.attrs.format || 'yaml';
        const content = current.attrs.content || '';
        const title =
          frontmatterFieldValue(content, 'title', format) || 'Untitled';
        const lang = frontmatterFieldValue(content, 'lang', format);
        const editable =
          editor.isEditable && frontmatterFieldsAreEditable(content, format);
        toggle.textContent = `Document metadata · ${title}${lang ? ` · ${lang}` : ''}`;
        dom.setAttribute('aria-label', 'Document metadata');
        rawLabelText.textContent = `Raw ${format.toUpperCase()}`;
        raw.value = content;
        raw.disabled = !editor.isEditable;
        for (const [key, input] of inputs) {
          input.value = frontmatterFieldValue(content, key, format);
          input.disabled = !editable;
        }
      };

      toggle.addEventListener('click', () => {
        body.hidden = !body.hidden;
        toggle.setAttribute('aria-expanded', String(!body.hidden));
        if (!body.hidden) inputs.get('title')?.focus();
      });
      for (const [key, input] of inputs) {
        input.addEventListener('change', () => {
          commit(
            setFrontmatterField(
              current.attrs.content || '',
              key,
              input.value,
              current.attrs.format || 'yaml',
            ),
          );
        });
      }
      raw.addEventListener('change', () => {
        const hasFence = /(^|\n)---(?:\s|$)/.test(raw.value);
        raw.setCustomValidity(
          hasFence
            ? 'Frontmatter content cannot contain its closing --- fence.'
            : '',
        );
        if (!hasFence) commit(raw.value);
        else raw.reportValidity();
      });
      render();

      return {
        dom,
        update: (updated) => {
          if (updated.type !== current.type) return false;
          current = updated;
          render();
          return true;
        },
        stopEvent: (event) => dom.contains(event.target as Node),
        ignoreMutation: (mutation) => dom.contains(mutation.target),
      };
    };
  },
});
