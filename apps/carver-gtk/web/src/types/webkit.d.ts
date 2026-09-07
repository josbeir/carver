import type { RichEditorApi, WebKitMessageHandler } from '../editor/protocol';

declare global {
  var webkit:
    | {
        messageHandlers?: { carver?: WebKitMessageHandler };
      }
    | undefined;

  var carverEditor: RichEditorApi | undefined;
}
