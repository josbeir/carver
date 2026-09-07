export type { EditorEvent, SelectionState } from './protocol.generated';

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
