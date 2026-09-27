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
      'Write in a rich editor with headings, checklists, links, and tables, all backed by canonical Carve source you can read at any time.',
    points: [
      'Keep your hands on the keyboard with familiar formatting shortcuts.',
      'Pasted images are kept as managed files beside your notes, never inside the database.',
      'Switch between writing, source, and preview without losing your place.',
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
      'Prefer plain text? Edit the canonical .crv source in a highlighted editor with line numbers and a breadcrumb trail of the block you are in, beside a live preview.',
    points: [
      'The rich editor, the source view, and every export share one Carve document.',
      'Import Carve or Markdown and keep a single format in your library.',
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
      'Saved views called Bases arrange notes by their frontmatter properties. Choose columns, filter and sort, and edit the values in place without opening a note.',
    points: [
      'Properties are typed — text, number, boolean, list, or date — so each cell gets the right editor.',
      'Edits from the grid are written straight back to the note, title overrides included.',
      'A Base is a view over your library, not a copy of it.',
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
      'Connect a compatible assistant through a local MCP server, so it can find and read your notes by opening the same library as the app.',
    points: [
      'Read-only by default; enable reversible note changes with a single flag.',
      'Works with Claude Code, Codex, GitHub Copilot, VS Code, OpenCode, or any stdio MCP client.',
      'Local by design: no listener, no telemetry, and no raw database or image access.',
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
      'Keep notes in categories that carry their own icon and colour, and pick up where you left off with recently opened notes.',
    points: [
      'Deleting a note is reversible: undo it, or restore it from Trash later.',
      'Your library is a local database, so it is quick to search and easy to back up.',
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
      'Search the whole library with full-text search, or find a phrase inside the note you have open.',
    points: [
      'Full-text search is handled by SQLite FTS5, so it stays quick as the library grows.',
      'Export to Carve, Markdown, or PDF, or bundle notes and images into a portable archive.',
    ],
    shot: shots.search,
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
