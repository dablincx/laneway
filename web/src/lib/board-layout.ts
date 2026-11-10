/**
 * The order of columns and cards on a board, as plain ids. Drag and drop works on this while
 * dragging and only tells the server about the final place.
 */
export type Layout = { columns: string[]; cards: Record<string, string[]> };

/** Builds the layout from the server's lists (both already sorted by position). */
export function layoutOf(columns: { id: string }[], cards: { id: string; column_id: string }[]): Layout {
  const layout: Layout = { columns: columns.map((c) => c.id), cards: {} };
  for (const column of columns) layout.cards[column.id] = [];
  for (const card of cards) layout.cards[card.column_id]?.push(card.id);
  return layout;
}

/** Where a card is: its column and its index in that column. */
export function placeOf(layout: Layout, cardId: string): { columnId: string; index: number } | null {
  for (const [columnId, ids] of Object.entries(layout.cards)) {
    const index = ids.indexOf(cardId);
    if (index !== -1) return { columnId, index };
  }
  return null;
}

/** Moves a card to `index` in `columnId` (past the end = last). Returns a new layout. */
export function moveCard(layout: Layout, cardId: string, columnId: string, index: number): Layout {
  const cards: Record<string, string[]> = {};
  for (const [id, ids] of Object.entries(layout.cards)) cards[id] = ids.filter((c) => c !== cardId);
  const target = cards[columnId];
  if (!target) return layout;
  target.splice(Math.max(0, Math.min(index, target.length)), 0, cardId);
  return { columns: layout.columns, cards };
}

/** Moves a column to `index` (past the end = last). Returns a new layout. */
export function moveColumn(layout: Layout, columnId: string, index: number): Layout {
  if (!layout.columns.includes(columnId)) return layout;
  const columns = layout.columns.filter((c) => c !== columnId);
  columns.splice(Math.max(0, Math.min(index, columns.length)), 0, columnId);
  return { columns, cards: layout.cards };
}
