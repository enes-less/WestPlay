/** An app on Home takes this much room at least, page pixels: its column's width, and its row's height (the
 * icon, the gap and the label: home.css). */
export const APP_WIDTH = 220
export const APP_HEIGHT = 198
/** Between rows (home.css `row-gap`). */
export const ROW_GAP = 54

/**
 * How many apps a page of `width` x `height` holds: as many columns as fit (4 on the full 16:9 screen, 5 in the
 * Scania Smart Dash's area at its scale) and as many rows (2 on either), never fewer than one each.
 */
export function pageSize(width: number, height: number): { columns: number; rows: number } {
  const columns = Math.max(1, Math.floor(width / APP_WIDTH))
  const rows = Math.max(1, Math.floor((height + ROW_GAP) / (APP_HEIGHT + ROW_GAP)))
  return { columns, rows }
}

/** The apps in pages of `perPage`, in order (one empty page without apps). */
export function pagesOf<T>(apps: readonly T[], perPage: number): T[][] {
  const pages: T[][] = []
  for (let at = 0; at < apps.length; at += Math.max(1, perPage)) pages.push(apps.slice(at, at + Math.max(1, perPage)))
  return pages.length > 0 ? pages : [[]]
}
