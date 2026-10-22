import { describe, expect, it } from 'vitest'
import { pagesOf, pageSize } from './pages'

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

  it('never has fewer than one column or row', () => {
    expect(pageSize(100, 100)).toEqual({ columns: 1, rows: 1, compact: true })
  })
})

describe('pagesOf', () => {
  it('fills each page in order, the rest on the next', () => {
    expect(pagesOf([1, 2, 3, 4, 5, 6, 7, 8, 9], 8)).toEqual([[1, 2, 3, 4, 5, 6, 7, 8], [9]])
    expect(pagesOf([1, 2, 3], 8)).toEqual([[1, 2, 3]])
    expect(pagesOf([], 8)).toEqual([[]])
  })
})
