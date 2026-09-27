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
    label: 'Rich editor',
    title: 'Write in a rich editor that respects your source',
    teaser: 'Format as you write, in rich text or Carve.',
    icon: 'pen',
    summary:
      'Format notes the way you think: headings, inline styling, lists, tasks, links, and tables, with keyboard shortcuts throughout.',
    points: [
      'Paste Carve directly, or use “Paste as Markdown” to migrate Markdown into canonical Carve.',
      'Paste, resize, and keep managed images beside the note, then browse them from the media sidebar.',
      'Switching between Edit, Source, and Preview never discards formatting or blank lines.',
    ],
    shot: shots.editor,
  },
  {
    id: 'source',
    label: 'Carve source',
    title: 'The source is yours, always readable',
    teaser: 'Read and edit the canonical markup.',
    icon: 'code',
    summary:
      'Drop into Carve source whenever you want full control. The source editor highlights markup, follows breadcrumbs, and searches within the note, while the preview stays read-only.',
    points: [
      'Canonical .crv source with a synchronized split view.',
      'Carve and Markdown import keep one consistent format instead of drifted syntax.',
    ],
    code: { label: 'week.crv', code: carveSample },
    shot: shots.source,
  },
  {
    id: 'bases',
    label: 'Saved Bases',
    title: 'Turn scattered notes into a view',
    teaser: "Views built from your notes' properties.",
    icon: 'table',
    summary:
      'Build database-style views from the frontmatter already in your notes: choose the columns, filter and sort the rows, and search across the result.',
    points: [
      'Frontmatter properties become sortable, filterable columns.',
      'The same notes, arranged the way the work needs them.',
    ],
    shot: shots.bases,
  },
  {
    id: 'agent',
    label: 'Agent access',
    title: 'Turn your notes into project context',
    teaser: 'Let a local AI client read your notes.',
    icon: 'sparkles',
    summary:
      'Expose your library to local AI clients through a stdio MCP server, so an agent can search and read the context you already wrote instead of starting from an empty chat.',
    points: [
      'Read-only by default; opt into reversible note changes with --allow-write.',
      'Works with Codex, Claude Code, GitHub Copilot, VS Code, OpenCode, or any stdio MCP client.',
      'Local only: no listener, no telemetry, and never managed asset bytes.',
    ],
    clients: [
      { name: 'Claude Code', logo: anthropic },
      { name: 'Codex', logo: codex },
      { name: 'Copilot', logo: copilot },
      { name: 'OpenCode', logo: opencode },
    ],
    code: { label: 'Set up a client', code: configureCommands },
    shot: shots.agent,
  },
  {
    id: 'library',
    label: 'Library & Trash',
    title: 'Organized, and hard to lose',
    teaser: 'Categories, recent notes, and Trash.',
    icon: 'folder',
    summary:
      'Group notes into categories, jump back in through recently opened notes, and recover anything you removed from the Trash.',
    points: [
      'Deleting a note soft deletes it and offers an Undo, so a slip never costs you writing.',
      'Categories carry an icon and accent colour; every counter stays in step after a move.',
    ],
    shot: shots.library,
  },
  {
    id: 'search',
    label: 'Search & export',
    title: 'Find anything, take it anywhere',
    teaser: 'Find anything, take it anywhere.',
    icon: 'search',
    summary:
      'Full-text search across the library and search within a note. Bring writing in from Carve or Markdown, and send it back out in the format you need.',
    points: [
      'Fast full-text search backed by SQLite FTS5.',
      'Export to Carve, Markdown, or PDF, or a portable archive for managed images.',
    ],
  },
  {
    id: 'preferences',
    label: 'Preferences',
    title: 'Set it up to match how you work',
    teaser: 'Tune the editor to your workflow.',
    icon: 'sliders',
    summary:
      'Choose your default editing mode, how source presentation behaves, whether remote images load, and which formatting controls appear.',
    points: [
      'Preferences are a plain TOML file under your XDG config directory.',
      'Light and dark themes with accessible, keyboard-friendly controls throughout.',
    ],
    shot: shots.settings,
  },
];
