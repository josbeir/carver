import anthropic from '../assets/agent-icons/anthropic.svg?raw';
import codex from '../assets/agent-icons/codex-openai.svg?raw';
import copilot from '../assets/agent-icons/copilot.svg?raw';
import opencode from '../assets/agent-icons/opencode.svg?raw';
import { shots, type Shot } from './screenshots';

/** A benefit group in the progressively enhanced feature tabs. */
export interface FeatureArea {
  id: string;
  label: string;
  title: string;
  summary: string;
  points: string[];
  shot?: Shot;
  clients?: { name: string; logo: string }[];
  note?: string;
}

export const featureAreas: FeatureArea[] = [
  {
    id: 'write',
    label: 'Write',
    title: 'Make room for your next idea.',
    summary:
      'Jot down a quick thought or settle into a longer draft. Write in rich text or edit Carve source, with images, checklists, and tables close at hand.',
    points: [
      'Rich text, source, and preview',
      'Adjust the tools and panels to suit your writing.',
      'Open the command palette with Ctrl+Shift+P to find commands and jump to notes.',
    ],
    shot: shots.source,
  },
  {
    id: 'organize',
    label: 'Organize',
    title: 'A little order goes a long way.',
    summary:
      'Group notes into categories, find them with search, and bring related work into saved views called Bases.',
    points: [
      'Filter and sort by your notes’ properties',
      'Edit property values directly in a saved view.',
    ],
    shot: shots.bases,
  },
  {
    id: 'connect',
    label: 'Connect',
    title: 'Bring your notes into the conversation.',
    summary:
      'Connect an assistant such as Claude Code or Codex so it can search and read your notes for useful context.',
    points: ['Optional, with read-only access by default.'],
    clients: [
      { name: 'Claude Code', logo: anthropic },
      { name: 'Codex', logo: codex },
      { name: 'Copilot', logo: copilot },
      { name: 'OpenCode', logo: opencode },
    ],
    note: 'Connected assistants may send note content to their AI provider.',
    shot: shots.agent,
  },
];
