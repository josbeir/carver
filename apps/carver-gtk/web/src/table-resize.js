import { addColumn, addRow, removeColumn, removeRow, selectedRect } from '@tiptap/pm/tables';

const MAX_ROWS = 4;
const MAX_COLUMNS = 6;

export function tableSize(rows, columns) {
  return {
    rows: Math.max(1, Math.min(MAX_ROWS, Number(rows) || 3)),
    columns: Math.max(1, Math.min(MAX_COLUMNS, Number(columns) || 3)),
  };
}

// Each table command must start from the transaction produced by its predecessor:
// prosemirror-tables maps positions from that command's input document. Returning
// the short sequence keeps the resize lossless while the host dispatches it in order.
export function resizeSelectedTable(state, { rows, columns, header }) {
  const target = tableSize(rows, columns);
  const transactions = [];
  let currentState = state;
  let rect = selectedTableRect(currentState);
  if (!rect) return transactions;

  while (rect.map.height < target.rows) {
    const height = rect.map.height;
    currentState = applyTableChange(currentState, transactions, current => {
      const currentRect = selectedTableRect(current);
      return addRow(current.tr, currentRect, currentRect.map.height);
    });
    rect = selectedTableRect(currentState);
    if (!rect || rect.map.height <= height) return transactions;
  }
  while (rect.map.height > target.rows) {
    const height = rect.map.height;
    currentState = applyTableChange(currentState, transactions, current => {
      const currentRect = selectedTableRect(current);
      const transaction = current.tr;
      removeRow(transaction, currentRect, currentRect.map.height - 1);
      return transaction;
    });
    rect = selectedTableRect(currentState);
    if (!rect || rect.map.height >= height) return transactions;
  }
  while (rect.map.width < target.columns) {
    const width = rect.map.width;
    currentState = applyTableChange(currentState, transactions, current => {
      const currentRect = selectedTableRect(current);
      return addColumn(current.tr, currentRect, currentRect.map.width);
    });
    rect = selectedTableRect(currentState);
    if (!rect || rect.map.width <= width) return transactions;
  }
  while (rect.map.width > target.columns) {
    const width = rect.map.width;
    currentState = applyTableChange(currentState, transactions, current => {
      const currentRect = selectedTableRect(current);
      const transaction = current.tr;
      removeColumn(transaction, currentRect, currentRect.map.width - 1);
      return transaction;
    });
    rect = selectedTableRect(currentState);
    if (!rect || rect.map.width >= width) return transactions;
  }
  const headerTransaction = currentState.tr;
  setFirstRowHeader(headerTransaction, rect, header);
  if (headerTransaction.docChanged) transactions.push(headerTransaction);
  return transactions;
}

function selectedTableRect(state) {
  try {
    return selectedRect(state);
  } catch {
    return null;
  }
}

function applyTableChange(state, transactions, change) {
  const transaction = change(state);
  transactions.push(transaction);
  return state.apply(transaction);
}

function setFirstRowHeader(transaction, rect, header) {
  const targetType = header
    ? rect.table.type.schema.nodes.tableHeader
    : rect.table.type.schema.nodes.tableCell;
  if (!targetType) return;
  const seen = new Set();
  for (let column = 0; column < rect.map.width; column += 1) {
    const cellOffset = rect.map.map[column];
    if (seen.has(cellOffset)) continue;
    seen.add(cellOffset);
    const cell = rect.table.nodeAt(cellOffset);
    if (cell?.type !== targetType) {
      transaction.setNodeMarkup(
        transaction.mapping.map(rect.tableStart + cellOffset),
        targetType,
        cell?.attrs,
      );
    }
  }
}
