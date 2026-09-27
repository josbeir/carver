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
       * Host document session.
       */
      session: number;
      /**
       * Canonical source for the selected document fragment.
       */
      source: string;
      type: 'copy-selection';
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
    }
  | {
      /**
       * Web-surface request identity echoed back with the imported source.
       */
      request_id: number;
      /**
       * Host document session.
       */
      session: number;
      /**
       * Raw pasted text.
       */
      text: string;
      type: 'paste-text';
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
   * Heading occurrence enclosing the current selection.
   */
  heading_occurrence: number | null;
  /**
   * Selected image width percentage, when an image is selected.
   */
  image_width: number | null;
  /**
   * Media at the current selection, if any.
   */
  media: MediaSelection | null;
  /**
   * Navigation sequence used to reject selection events preceding a host focus request.
   */
  navigation_epoch: number;
  /**
   * Content revision within this projection's load session.
   */
  revision: number;
  /**
   * Table enclosing the current selection, if any.
   */
  table: TableSelection | null;
}
/**
 * One media occurrence in an editor projection.
 */
export interface MediaSelection {
  /**
   * Zero-based occurrence among references to the same path.
   */
  occurrence: number;
  /**
   * Authored image source or attachment destination.
   */
  path: string;
}
/**
 * Geometry of the table enclosing the current selection.
 *
 * The host uses this to reflect the live table structure in its native size
 * picker instead of guessing from the last inserted dimensions.
 */
export interface TableSelection {
  /**
   * Number of columns.
   */
  columns: number;
  /**
   * Whether the first row is a header.
   */
  header: boolean;
  /**
   * Total number of rows, including the optional header row.
   *
   * Kept wider than the `u8` insert command: a table can grow past 255 rows
   * through ordinary row insertion, and an out-of-range value would make the
   * whole selection event fail to deserialize.
   */
  rows: number;
}

/**
 * An occurrence addressed independently of an editor's position representation.
 */
export type DocumentTarget =
  | {
      kind: 'heading';
      /**
       * Zero-based heading occurrence.
       */
      occurrence: number;
    }
  | {
      kind: 'media';
      /**
       * Zero-based occurrence among references to this path.
       */
      occurrence: number;
      /**
       * Authored asset path.
       */
      path: string;
    };

/**
 * Dimensions for inserting or resizing a rich-editor table.
 */
export interface TableCommand {
  /**
   * Number of columns.
   */
  columns: number;
  /**
   * Whether the first row is a header.
   */
  header: boolean;
  /**
   * Total number of rows, including the optional header row.
   */
  rows: number;
}

/**
 * A labelled link to insert over the current selection.
 */
export interface LinkCommand {
  /**
   * Link destination.
   */
  destination: string;
  /**
   * Visible link text.
   */
  text: string;
}
