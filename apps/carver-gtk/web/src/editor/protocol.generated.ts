// Generated from carver-editor-protocol. Run npm run protocol:generate; do not edit.

/**
 * Events emitted by an editing surface.
 */
export type EditorEvent =
  | {
      type: 'ready';
    }
  | {
      /**
       * Monotonically increasing revision within the session.
       */
      revision: number;
      /**
       * Monotonically increasing host session identifier.
       */
      session: number;
      /**
       * Serialized source.
       */
      source: string;
      type: 'changed';
    }
  | {
      /**
       * Host document session.
       */
      session: number;
      /**
       * Selected state.
       */
      state: SelectionState;
      type: 'selection';
    }
  | {
      /**
       * Nodes whose original shape would degrade.
       */
      degraded: string[];
      /**
       * Host document session.
       */
      session: number;
      type: 'unsupported';
      /**
       * Unsupported node names.
       */
      unsupported: string[];
    }
  | {
      /**
       * Base64-encoded image bytes.
       */
      data: string;
      /**
       * Browser-reported media type.
       */
      mime_type: string;
      /**
       * Host document session.
       */
      session: number;
      type: 'paste-image';
    };

/**
 * Selection information used to reflect state in host-native controls.
 */
export interface SelectionState {
  /**
   * Active formatting identifiers.
   */
  active: string[];
  /**
   * Heading level at the selection, or zero for a paragraph.
   */
  heading: number;
  /**
   * Selected image width percentage, when an image is selected.
   */
  image_width: number | null;
}
