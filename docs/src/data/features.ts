import anthropic from '../assets/agent-icons/anthropic.svg?raw';
import codex from '../assets/agent-icons/codex-openai.svg?raw';
import copilot from '../assets/agent-icons/copilot.svg?raw';
import opencode from '../assets/agent-icons/opencode.svg?raw';
import { shots, type Shot } from './screenshots';

/** Inline SVG path markup keyed by feature, rendered in the rail and heading. */
export const icons = {
  pen: '<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"/>',
  code: '<path d="m16 18 6-6-6-6"/><path d="m8 6-6 6 6 6"/>',
  table: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M3 15h18M9 3v18"/>',
  sparkles:
    '<path d="M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5 18 18M18 6l-2.5 2.5M8.5 15.5 6 18"/>',
  folder:
    '<path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7l-2-2H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2Z"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  sliders: '<path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6"/>',
} as const;

export type IconName = keyof typeof icons;

export interface FeatureArea {
  /** Stable id, also used as the radio value and panel key. */
  id: string;
  /** Title shown in the rail. */
  label: string;
  /** Longer heading shown above the detail. */
  title: string;
  /** One-line description shown under the rail title. */
  teaser: string;
  icon: IconName;
  summary: string;
  points: string[];
  /** Optional themed screenshot; the detail renders text-only while pending. */
  shot?: Shot;
  /** Optional client chips, used by the agent-access area. */
  clients?: { name: string; logo: string }[];
  /** Optional code sample shown under the copy. */
  code?: { label: string; code: string };
}

const carveSample = `# Weekly review

- [x] Triage the inbox
- [ ] Draft the release notes

> Aim for a *calm* week: fewer, sharper tasks.

| Day | Focus   |
| --- | ------- |
| Mon | Writing |
| Tue | Reviews |`;

const configureCommands = [
  'carver-mcp configure claude-code',
  'carver-mcp configure codex',
  'carver-mcp configure copilot',
  'carver-mcp configure vscode',
  'carver-mcp configure opencode',
  'carver-mcp configure generic',
].join('\n');

export const featureAreas: FeatureArea[] = [
  {
    id: 'editor',
    label: 'A place to write',
    title: 'Get the thought down. Make it your own.',
    teaser: 'Quick notes, long drafts, and everything between.',
    icon: 'pen',
    summary:
      'Start with a few words and see where they take you. Add headings, checklists, links, and tables to give your ideas shape as you go.',
    points: [
      'Keep your hands on the keyboard with familiar formatting shortcuts.',
      'Paste images into your notes and resize them to fit your story.',
      'Switch between writing, source editing, and preview as you work.',
    ],
    shot: shots.editor,
  },
  {
    id: 'source',
    label: 'Write your way',
    title: 'Another way to put your thoughts on the page',
    teaser: 'Prefer plain text? Make yourself at home.',
    icon: 'code',
    summary:
      'For those who enjoy writing in plain text, Carver offers a source editor using Carve markup. See your words and their formatted preview side by side.',
    points: [
      'Move between the rich editor and source view without losing your work.',
      'Bring in existing Carve or Markdown notes and keep writing.',
    ],
    code: { label: 'week.crv', code: carveSample },
    shot: shots.source,
  },
  {
    id: 'bases',
    label: 'See the bigger picture',
    title: 'Give your notes a new perspective',
    teaser: 'Bring related ideas into one view.',
    icon: 'table',
    summary:
      'Turn a collection of notes into an overview you can work with. Saved views, called Bases, let you arrange notes by their properties to keep projects and plans in sight.',
    points: [
      'Choose the columns that matter, then filter and sort to narrow your focus.',
      'Explore your notes together without moving or duplicating them.',
    ],
    shot: shots.bases,
  },
  {
    id: 'agent',
    label: 'Connect your AI tools',
    title: 'Start the conversation with your ideas',
    teaser: 'Give your assistant useful context.',
    icon: 'sparkles',
    summary:
      'Connect a compatible AI assistant so it can find and read your notes. Bring your research and project context into the conversation without copying it all by hand.',
    points: [
      'Optional to set up, with read-only access by default. You decide whether to enable changes.',
      'Works with tools such as Claude Code, Codex, GitHub Copilot, and OpenCode.',
      'Your connected assistant may send note content to its AI provider as part of a conversation.',
    ],
    clients: [
      { name: 'Claude Code', logo: anthropic },
      { name: 'Codex', logo: codex },
      { name: 'Copilot', logo: copilot },
      { name: 'OpenCode', logo: opencode },
    ],
    code: { label: 'Connect your preferred assistant', code: configureCommands },
    shot: shots.agent,
  },
  {
    id: 'library',
    label: 'A little more order',
    title: 'A home for every thought',
    teaser: 'Keep your notes easy to come back to.',
    icon: 'folder',
    summary:
      'Gather notes into categories that make sense to you, from work projects to weekend plans. Recently opened notes help you pick up where you left off.',
    points: [
      'Give categories their own icons and colours so they are easy to spot.',
      'Changed your mind? Undo a deletion or restore a note from Trash.',
    ],
    shot: shots.library,
  },
  {
    id: 'search',
    label: 'Find it. Take it with you.',
    title: 'Good ideas deserve to be found again',
    teaser: 'Less searching, more picking up where you left off.',
    icon: 'search',
    summary:
      'Find that useful detail across your library or inside a longer note. When your writing is ready for its next stop, export it in a format that fits.',
    points: [
      'Search the words inside your notes, even when the title escapes you.',
      'Export as Markdown, PDF, or Carve, or bundle notes and images in a portable archive.',
    ],
  },
  {
    id: 'preferences',
    label: 'Make it yours',
    title: 'Settle into your own writing rhythm',
    teaser: 'A comfortable space, your way.',
    icon: 'sliders',
    summary:
      'Choose a light or dark theme, your preferred editing mode, and the formatting tools you want close at hand.',
    points: [
      'Open notes in the editing mode that feels right for you.',
      'Choose whether images from the web load in your notes.',
    ],
    shot: shots.settings,
  },
];
