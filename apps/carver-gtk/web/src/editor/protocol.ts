export interface WebKitMessageHandler {
  postMessage(message: string): void;
}

export interface RichEditorApi {
  load(source: string, session: number): boolean;
  command(name: string, argument?: unknown): boolean;
  source(): string;
  linkContext(): LinkContext;
  insertImage(path: string, alt?: string): void;
  setTheme(
    dark: boolean,
    accent: string,
    selectionBackground: string,
    selectionForeground: string,
  ): void;
}

export interface LinkContext {
  text: string;
  destination: string;
}

export interface TableCommand {
  rows?: number;
  columns?: number;
  header?: boolean;
}

export interface LinkCommand extends LinkContext {}

export type EditorEvent =
  | { type: 'ready' }
  | { type: 'changed'; session: number; revision: number; source: string }
  | { type: 'paste-image'; session: number; mime_type: string; data: string }
  | {
      type: 'unsupported';
      session: number;
      unsupported: string[];
      degraded: string[];
    }
  | {
      type: 'selection';
      session: number;
      state: { active: string[]; heading: number; image_width: number | null };
    };
