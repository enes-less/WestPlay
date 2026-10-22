/** An app on Home takes this much room at least, page pixels: its column's width, and its row's height (the
 * icon, the gap and the label: home.css), and the gap between rows (`row-gap`); and the same, compact (a
 * short screen's: `.home-pages.compact`). */
export const CELL = { width: 220, height: 198, rowGap: 54 }
export const COMPACT_CELL = { width: 136, height: 124, rowGap: 22 }

/**
 * How many apps a page of `width` x `height` holds: as many columns as fit (4 on the full 16:9 screen, 5 in the
 * Scania Smart Dash's area at its scale) and as many rows (2 on either), never fewer than one each. Where the
 * full-size cells don't fit two rows (the 7" Scania's screen, its UI scaled up), compact ones (4 x 2 there).
 */
export function pageSize(width: number, height: number): { columns: number; rows: number; compact: boolean } {
  const compact = rowsOf(CELL, height) < 2
  const cell = compact ? COMPACT_CELL : CELL
  return { columns: Math.max(1, Math.floor(width / cell.width)), rows: Math.max(1, rowsOf(cell, height)), compact }
}

function rowsOf(cell: typeof CELL, height: number): number {
  return Math.floor((height + cell.rowGap) / (cell.height + cell.rowGap))
}

/** The apps in pages of `perPage`, in order (one empty page without apps). */
export function pagesOf<T>(apps: readonly T[], perPage: number): T[][] {
  const pages: T[][] = []
  for (let at = 0; at < apps.length; at += Math.max(1, perPage)) pages.push(apps.slice(at, at + Math.max(1, perPage)))
  return pages.length > 0 ? pages : [[]]
}
