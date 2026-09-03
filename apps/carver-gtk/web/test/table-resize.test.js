import assert from 'node:assert/strict';
import test from 'node:test';

import { getSchema } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { Table, TableCell, TableHeader, TableRow } from '@tiptap/extension-table';
import { EditorState, TextSelection } from '@tiptap/pm/state';

import { resizeSelectedTable, tableSize } from '../src/table-resize.js';

function stateWithTable(rows, columns, header = true) {
  const schema = getSchema([StarterKit, Table, TableRow, TableHeader, TableCell]);
  const headerCell = schema.nodes.tableHeader.createAndFill();
  const bodyCell = schema.nodes.tableCell.createAndFill();
  const tableRows = Array.from({ length: rows }, (_, row) => schema.nodes.tableRow.create(
    null,
    Array.from({ length: columns }, () => (row === 0 && header ? headerCell : bodyCell)),
  ));
  const table = schema.nodes.table.create(null, tableRows);
  const document = schema.topNodeType.create(null, [table]);
  return EditorState.create({
    schema,
    doc: document,
    selection: TextSelection.create(document, 4),
  });
}

function applyTransactions(state, transactions) {
  return transactions.reduce((current, transaction) => current.apply(transaction), state);
}

test('clamps table picker dimensions to its visible grid', () => {
  assert.deepEqual(tableSize(20, 0), { rows: 4, columns: 3 });
});

test('resizes the selected table and applies the header switch atomically', () => {
  const state = stateWithTable(2, 2, true);
  const transactions = resizeSelectedTable(state, { rows: 4, columns: 5, header: false });
  assert.ok(transactions.length);
  const table = applyTransactions(state, transactions).doc.firstChild;
  assert.equal(table.childCount, 4);
  assert.equal(table.firstChild.childCount, 5);
  assert.equal(table.firstChild.firstChild.type.name, 'tableCell');
});

test('removes trailing table dimensions without deleting the table', () => {
  const state = stateWithTable(4, 5, false);
  const transactions = resizeSelectedTable(state, { rows: 2, columns: 2, header: true });
  assert.ok(transactions.length);
  const table = applyTransactions(state, transactions).doc.firstChild;
  assert.equal(table.childCount, 2);
  assert.equal(table.firstChild.childCount, 2);
  assert.equal(table.firstChild.firstChild.type.name, 'tableHeader');
});

test('resizes a table again after its previous resize', () => {
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
  assert.equal(table.childCount, 3);
  assert.equal(table.firstChild.childCount, 3);
  assert.equal(table.firstChild.firstChild.type.name, 'tableCell');
});
