import { describe, expect, it } from 'vitest'
import { CELL, cellHeight, pagesOf, pageSize, rowLayout } from './pages'

describe('pageSize', () => {
  it('fits 4 x 2 on the DAF and 5 x 2 on the Scania Smart Dash', () => {
    // The space right of the sidebar: 1078 x 656 on the 16:9 screen; 1114 x 638 or so at the Scania's scale.
    expect(pageSize(1078, 656)).toEqual({ columns: 4, rows: 2, compact: false })
    expect(pageSize(1114, 638)).toEqual({ columns: 5, rows: 2, compact: false })
  })

  it('goes compact where full-size cells don’t fit two rows: 4 x 2 on the 7" Scania', () => {
    // Its area at its scale: about 551 x 402.
    expect(pageSize(551, 402)).toEqual({ columns: 4, rows: 2, compact: true })
  })

  it('makes room for the larger names of a larger text size', () => {
    // The cell's height as Home's CSS lays it out at the designed text.
    expect(pageSize(1078, 656, 1)).toEqual({ columns: 4, rows: 2, compact: false })
    // The default Text Size (1.24): still 4 x 2 on the DAF, room for names of two lines; twice the text: 3 columns.
    expect(pageSize(1078, 656, 1.24)).toEqual({ columns: 4, rows: 2, compact: false })
    expect(pageSize(1078, 656, 2)).toEqual({ columns: 3, rows: 2, compact: false })
  })

  it('keeps the icons full size at any text size: compact by the screen alone', () => {
    // The DAF's Home at twice the text (1034 x 553, the sidebar wider): its full-size icons, not the compact ones.
    expect(pageSize(1034, 553, 2)).toEqual({ columns: 3, rows: 2, compact: false })
    expect(pageSize(551, 402, 2).compact).toBe(true)
  })

  it('never has fewer than one column or row', () => {
    expect(pageSize(100, 100)).toEqual({ columns: 1, rows: 1, compact: true })
  })
})

describe('rowLayout', () => {
  // Home's seven apps; YouTube Music's name on two lines.
  const names = [1, 1, 1, 1, 2, 1, 1]

  it('makes each row as tall as its names, the gap narrowed to keep the rows', () => {
    // The DAF at twice the text, 3 columns: YouTube Music's row a line taller, the gap 21 rather than one row.
    expect(rowLayout(CELL, 2, 553, 3, 2, names)).toEqual({
      rows: 2,
      heights: [cellHeight(CELL, 2), cellHeight(CELL, 2, 2)],
      gap: 21,
    })
    // Names of one line, as designed: the full gap.
    expect(rowLayout(CELL, 1, 553, 4, 2, [1, 1, 1, 1, 1, 1, 1])).toEqual({ rows: 2, heights: [198, 198], gap: 54 })
  })

  it('takes a row fewer only where even the narrowest gap is too much', () => {
    expect(rowLayout(CELL, 2, 500, 3, 2, names).rows).toBe(1)
  })

  it('has no more rows than the apps fill', () => {
    expect(rowLayout(CELL, 1, 900, 4, 3, names).rows).toBe(2)
  })
})

describe('pagesOf', () => {
  it('fills each page in order, the rest on the next', () => {
    expect(pagesOf([1, 2, 3, 4, 5, 6, 7, 8, 9], 8)).toEqual([[1, 2, 3, 4, 5, 6, 7, 8], [9]])
    expect(pagesOf([1, 2, 3], 8)).toEqual([[1, 2, 3]])
    expect(pagesOf([], 8)).toEqual([[]])
  })
})
