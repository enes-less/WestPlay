import { describe, expect, it } from 'vitest'
import { barLayout, CIRCLE, framed, GAP, INNER } from './barLayout'

const sizes = { bar: 1000, start: 0, end: 150, tabs: 500 }

describe('the Music bar', () => {
  it('centres the tabs and the search circle together', () => {
    const { tabs, search } = barLayout({ ...sizes, end: 0 }, false)
    const group = 500 + INNER + CIRCLE
    expect(tabs).toEqual({ left: (1000 - group) / 2, width: 500 })
    expect(search).toEqual({ left: tabs.left + 500 + INNER, width: CIRCLE })
  })

  it('keeps them clear of what is at the sides', () => {
    // Centred, the search circle would run into Sign In: moved left just enough.
    const { search } = barLayout({ ...sizes, tabs: 600 }, false)
    expect(search.left + search.width).toBe(1000 - 150 - GAP)
    // Back on the left of a narrow bar: never over it, even if the right is then reached.
    const narrow = barLayout({ bar: 700, start: 64, end: 150, tabs: 500 }, false)
    expect(narrow.tabs.left).toBe(64 + GAP)
  })

  it('draws each piece at its larger place, slid and cut to the one it shows', () => {
    const closed = barLayout(sizes, false)
    const open = barLayout(sizes, true)
    // The tab bar: laid out as the bar, shown as the circle at the left.
    expect(framed(closed.tabs, open.tabs)).toEqual({
      left: closed.tabs.left,
      width: closed.tabs.width,
      shift: open.tabs.left - closed.tabs.left,
      cut: closed.tabs.width - CIRCLE,
    })
    expect(framed(closed.tabs, closed.tabs)).toMatchObject({ shift: 0, cut: 0 })
    // The field: laid out open, shown as the circle where it sits closed.
    expect(framed(open.search, closed.search)).toMatchObject({
      left: open.search.left,
      shift: closed.search.left - open.search.left,
      cut: open.search.width - CIRCLE,
    })
  })

  it('folds the tabs into a circle and gives the field the room, searching', () => {
    const { tabs, search } = barLayout(sizes, true)
    expect(tabs).toEqual({ left: 0, width: CIRCLE })
    expect(search.left).toBe(CIRCLE + INNER)
    expect(search.left + search.width).toBe(1000 - 150 - GAP)
    // Clear of Back.
    expect(barLayout({ ...sizes, start: 64 }, true).tabs.left).toBe(64 + GAP)
    // The account hidden while typing: the field runs to the end.
    const typing = barLayout({ ...sizes, end: 0 }, true)
    expect(typing.search.left + typing.search.width).toBe(1000)
  })
})
