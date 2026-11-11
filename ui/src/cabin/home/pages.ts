/** An app on Home takes this much room at least, page pixels: its column's width, and its row's parts (the icon, the
 * gap under it, the label's padding and its lines: home.css), and the gap between rows, narrowed to `minRowGap` at the
 * most to keep a row; and the same, compact (a short screen's: `.home-pages.compact`). The label's lines grow with the
 * text size, and the column with it at half that rate; a long name goes onto a second line (owner 2026-10-24: "App
 * labels can be 2 lines"). The icons stay as they are at any text size: they're seen, not read. */
export const CELL = { width: 220, icon: 138, gap: 14, labelPadding: 12, labelLine: 34, rowGap: 54, minRowGap: 20 }
export const COMPACT_CELL = { width: 136, icon: 84, gap: 8, labelPadding: 8, labelLine: 24, rowGap: 22, minRowGap: 12 }

type Cell = typeof CELL

/** The lines a label may take. */
export const LABEL_LINES = 2

/** The most rows a page has: CarPlay's home on a car's wide screen is two rows, more apps on the next page (owner
 * 2026-11-11: the Smart Dash's taller area took three, "too crowded"). */
export const MOST_ROWS = 2

/** A cell's height at `textScale`, its label `lines` long. */
export function cellHeight(cell: Cell, textScale: number, lines = 1): number {
  return Math.ceil(cell.icon + cell.gap + cell.labelPadding + cell.labelLine * lines * textScale)
}

/** A cell's width at `textScale`. */
function cellWidth(cell: Cell, textScale: number): number {
  return cell.width * (1 + (textScale - 1) / 2)
}

/**
 * How many apps a page of `width` x `height` holds at `textScale` (the player's Text Size): as many columns as fit (4
 * on the full 16:9 screen, 5 in the Scania Smart Dash's area at its scale) and as many rows of names of one line as fit,
 * two at most ([`MOST_ROWS`]: CarPlay's), never fewer than one each; `rowLayout` fits the names as they are into them. Compact where the full-size
 * cells don't fit two rows at the text as designed (the 7" Scania's screen, its UI scaled up: 4 x 2): by the screen
 * alone, never by the text, so a larger text never makes the icons smaller.
 */
export function pageSize(
  width: number,
  height: number,
  textScale = 1,
): { columns: number; rows: number; compact: boolean } {
  const compact = rowsOf(CELL, height, 1) < 2
  const cell = compact ? COMPACT_CELL : CELL
  return {
    columns: Math.max(1, Math.floor(width / cellWidth(cell, textScale))),
    rows: Math.max(1, Math.min(MOST_ROWS, rowsOf(cell, height, textScale))),
    compact,
  }
}

function rowsOf(cell: Cell, height: number, textScale: number): number {
  return Math.floor((height + cell.rowGap) / (cellHeight(cell, textScale) + cell.rowGap))
}

/**
 * The rows of Home's pages: how many (at most `rows`, as `pageSize` fits, and no more than the apps fill), each as tall
 * as the longest name in it on any page needs (`lines`: each app's label's lines, in Home's order), and the gap between
 * them, narrowed rather than a row fewer (a name of two lines in a row of `height` that held two rows of one). Every
 * page has the same rows.
 */
export function rowLayout(
  cell: Cell,
  textScale: number,
  height: number,
  columns: number,
  rows: number,
  lines: readonly number[],
): { rows: number; heights: number[]; gap: number } {
  for (let count = Math.max(1, Math.min(rows, Math.ceil(lines.length / columns))); ; count--) {
    const heights = Array.from({ length: count }, (_, row) => {
      let most = 1
      for (let start = row * columns; start < lines.length; start += count * columns) {
        for (const name of lines.slice(start, start + columns)) most = Math.max(most, name)
      }
      return cellHeight(cell, textScale, Math.min(LABEL_LINES, most))
    })
    const room = count > 1 ? (height - heights.reduce((sum, row) => sum + row, 0)) / (count - 1) : cell.rowGap
    if (count === 1 || room >= cell.minRowGap) {
      return { rows: count, heights, gap: Math.floor(Math.min(cell.rowGap, room)) }
    }
  }
}

/** The apps in pages of `perPage`, in order (one empty page without apps). */
export function pagesOf<T>(apps: readonly T[], perPage: number): T[][] {
  const pages: T[][] = []
  for (let at = 0; at < apps.length; at += Math.max(1, perPage)) pages.push(apps.slice(at, at + Math.max(1, perPage)))
  return pages.length > 0 ? pages : [[]]
}

/** A flick: let go at this speed or faster (page pixels per millisecond), the way it was dragged, it turns the page
 * however short the drag. */
export const FLICK_SPEED = 0.35

/** How far past the first or last page a drag goes, of how far the pointer moved (it gives, and springs back). */
export const EDGE_GIVE = 0.35

/**
 * The page a drag let go of settles on, from page `page` of `count`: the next one the way it went once it's past half
 * the page's `width` (`offset`: how far the pages were dragged, page pixels, negative to the left: on to the next),
 * or flicked that way (`speed`, page pixels per millisecond, as the pointer moved); else the same page. Never past
 * the first or the last.
 */
export function settlePage(page: number, count: number, offset: number, width: number, speed: number): number {
  if (offset === 0) return page
  const onward = offset < 0 ? 1 : -1
  const flicked = Math.abs(speed) >= FLICK_SPEED && Math.sign(speed) === Math.sign(offset)
  const past = Math.abs(offset) > width / 2
  return Math.max(0, Math.min(count - 1, page + (past || flicked ? onward : 0)))
}
