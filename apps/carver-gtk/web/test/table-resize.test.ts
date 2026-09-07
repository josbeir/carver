import { describe, expect, it } from 'vitest';

import { getSchema } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {
  Table,
  TableCell,
  TableHeader,
  TableRow,
} from '@tiptap/extension-table';
import { EditorState, TextSelection } from '@tiptap/pm/state';

import { resizeSelectedTable, tableSize } from '../src/editor/table-resize';

function stateWithTable(rows, columns, header = true) {
  const schema = getSchema([
    StarterKit,
    Table,
    TableRow,
    TableHeader,
    TableCell,
  ]);
  const headerCell = schema.nodes.tableHeader.createAndFill();
  const bodyCell = schema.nodes.tableCell.createAndFill();
  const tableRows = Array.from({ length: rows }, (_, row) =>
    schema.nodes.tableRow.create(
      null,
      Array.from({ length: columns }, () =>
        row === 0 && header ? headerCell : bodyCell,
      ),
    ),
  );
  const table = schema.nodes.table.create(null, tableRows);
  const document = schema.topNodeType.create(null, [table]);
  return EditorState.create({
    schema,
    doc: document,
    selection: TextSelection.create(document, 4),
  });
}

function applyTransactions(state, transactions) {
  return transactions.reduce(
    (current, transaction) => current.apply(transaction),
    state,
  );
}

describe('table resizing', () => {
  it('clamps table picker dimensions to its visible grid', () => {
    expect(tableSize(20, 0)).toEqual({ rows: 4, columns: 3 });
    expect(tableSize(-3, '5')).toEqual({ rows: 1, columns: 5 });
  });

  it('does nothing when the current selection is outside a table', () => {
    const schema = getSchema([
      StarterKit,
      Table,
      TableRow,
      TableHeader,
      TableCell,
    ]);
    const document = schema.topNodeType.create(null, [
      schema.nodes.paragraph.createAndFill(),
    ]);
    const state = EditorState.create({ schema, doc: document });
    expect(
      resizeSelectedTable(state, { rows: 2, columns: 2, header: true }),
    ).toEqual([]);
  });

  it('resizes the selected table and applies the header switch atomically', () => {
    const state = stateWithTable(2, 2, true);
    const transactions = resizeSelectedTable(state, {
      rows: 4,
      columns: 5,
      header: false,
    });
    expect(transactions).not.toHaveLength(0);
    const table = applyTransactions(state, transactions).doc.firstChild;
    expect(table.childCount).toBe(4);
    expect(table.firstChild?.childCount).toBe(5);
    expect(table.firstChild?.firstChild?.type.name).toBe('tableCell');
  });

  it('removes trailing table dimensions without deleting the table', () => {
    const state = stateWithTable(4, 5, false);
    const transactions = resizeSelectedTable(state, {
      rows: 2,
      columns: 2,
      header: true,
    });
    expect(transactions).not.toHaveLength(0);
    const table = applyTransactions(state, transactions).doc.firstChild;
    expect(table.childCount).toBe(2);
    expect(table.firstChild?.childCount).toBe(2);
    expect(table.firstChild?.firstChild?.type.name).toBe('tableHeader');
  });

  it('resizes a table again after its previous resize', () => {
    const initial = stateWithTable(2, 2, true);
    const expanded = applyTransactions(
      initial,
      resizeSelectedTable(initial, { rows: 4, columns: 5, header: true }),
    );
    const shrunk = applyTransactions(
      expanded,
      resizeSelectedTable(expanded, { rows: 3, columns: 3, header: false }),
    );
    const table = shrunk.doc.firstChild;
    expect(table.childCount).toBe(3);
    expect(table.firstChild?.childCount).toBe(3);
    expect(table.firstChild?.firstChild?.type.name).toBe('tableCell');
  });
});
