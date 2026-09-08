export interface SceneGridPlacementInput {
  nodeId: string;
  columnStart?: number;
  rowStart?: number;
  columnSpan?: number;
  rowSpan?: number;
}

export interface SceneGridPlacementResult {
  nodeId: string;
  column: number;
  row: number;
  columnSpan: number;
  rowSpan: number;
  collision: boolean;
  outOfBounds: boolean;
}

function cellKey(row: number, column: number): string {
  return `${row}:${column}`;
}

function fits(
  occupied: Set<string>,
  row: number,
  column: number,
  rowSpan: number,
  columnSpan: number,
  columnCount: number
): boolean {
  if (row < 0 || column < 0 || column + columnSpan > columnCount) return false;
  for (let rowOffset = 0; rowOffset < rowSpan; rowOffset += 1) {
    for (let columnOffset = 0; columnOffset < columnSpan; columnOffset += 1) {
      if (occupied.has(cellKey(row + rowOffset, column + columnOffset))) return false;
    }
  }
  return true;
}

function occupy(occupied: Set<string>, placement: SceneGridPlacementResult): void {
  for (let rowOffset = 0; rowOffset < placement.rowSpan; rowOffset += 1) {
    for (let columnOffset = 0; columnOffset < placement.columnSpan; columnOffset += 1) {
      occupied.add(cellKey(placement.row + rowOffset, placement.column + columnOffset));
    }
  }
}

export function placeSceneGridItems(
  items: SceneGridPlacementInput[],
  columnCount: number,
  autoFlow: 'row' | 'column' | 'dense',
  explicitRowCount = 0
): SceneGridPlacementResult[] {
  if (!Number.isSafeInteger(columnCount) || columnCount < 1) throw new Error('Grid column count is invalid.');
  const occupied = new Set<string>();
  const results = new Map<string, SceneGridPlacementResult>();
  let cursorRow = 0;
  let cursorColumn = 0;

  function findPosition(item: SceneGridPlacementInput): { row: number; column: number } {
    const rowSpan = item.rowSpan ?? 1;
    const columnSpan = item.columnSpan ?? 1;
    const fixedRow = item.rowStart === undefined ? undefined : item.rowStart - 1;
    const fixedColumn = item.columnStart === undefined ? undefined : item.columnStart - 1;
    if (fixedRow !== undefined && fixedColumn !== undefined) return { row: fixedRow, column: fixedColumn };
    const dense = autoFlow === 'dense';
    const maximumAttempts = Math.max(1024, items.length * columnCount * 8);
    let row = dense ? 0 : cursorRow;
    let column = dense ? 0 : cursorColumn;
    if (fixedRow !== undefined) row = fixedRow;
    if (fixedColumn !== undefined) column = fixedColumn;
    for (let attempt = 0; attempt < maximumAttempts; attempt += 1) {
      if (fits(occupied, row, column, rowSpan, columnSpan, columnCount)) return { row, column };
      if (fixedRow !== undefined) {
        column += 1;
        if (column >= columnCount) break;
      } else if (fixedColumn !== undefined) {
        row += 1;
      } else if (autoFlow === 'column') {
        row += 1;
        const rowLimit = Math.max(1, explicitRowCount, Math.ceil(items.length / columnCount));
        if (row >= rowLimit) {
          row = 0;
          column += 1;
          if (column >= columnCount) {
            column = 0;
            row = rowLimit;
          }
        }
      } else {
        column += 1;
        if (column >= columnCount) {
          column = 0;
          row += 1;
        }
      }
    }
    return { row: Math.max(0, row), column: Math.max(0, Math.min(column, columnCount - 1)) };
  }

  const ordered = [...items.filter((item) => item.rowStart !== undefined && item.columnStart !== undefined), ...items.filter((item) => item.rowStart === undefined || item.columnStart === undefined)];
  for (const item of ordered) {
    const rowSpan = item.rowSpan ?? 1;
    const columnSpan = item.columnSpan ?? 1;
    const position = findPosition(item);
    const outOfBounds = position.column < 0 || position.row < 0 || position.column + columnSpan > columnCount;
    const collision = !outOfBounds && !fits(occupied, position.row, position.column, rowSpan, columnSpan, columnCount);
    const result = { nodeId: item.nodeId, ...position, rowSpan, columnSpan, collision, outOfBounds };
    results.set(item.nodeId, result);
    if (!outOfBounds) occupy(occupied, result);
    if (autoFlow !== 'dense' && item.rowStart === undefined && item.columnStart === undefined) {
      cursorRow = position.row;
      cursorColumn = position.column + columnSpan;
      while (cursorColumn >= columnCount) {
        cursorColumn -= columnCount;
        cursorRow += 1;
      }
    }
  }
  return items.map((item) => results.get(item.nodeId)!);
}
