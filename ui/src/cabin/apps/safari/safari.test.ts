import { describe, expect, it } from 'vitest'
import { frequentlyVisited, historyMatches, tabName, timeframeStart, visited } from './safari'
import { typedSteps } from './typed'
import { pageColours, rgbOf } from './theme'
import { outerShadow } from './hole'

describe('Safari', () => {
  it('casts on the page the outer shadows of what covers it, at the cab’s scale', () => {
    // The keyboard's: its rim inset (not cast), its two shadows (cast), at the UI scale 1.5.
    const keyboard =
      'rgba(255, 255, 255, 0.5) 0px 1px 0px 0px inset, rgba(0, 0, 0, 0.12) 0px 1.5px 3px 0px, rgba(0, 0, 0, 0.18) 0px 14px 36px 0px'
    expect(outerShadow(keyboard, 1.5)).toBe(
      'rgba(0, 0, 0, 0.12) 0px 2.25px 4.5px 0px, rgba(0, 0, 0, 0.18) 0px 21px 54px 0px',
    )
    expect(outerShadow('rgba(0, 0, 0, 0.3) 0px 0px 0px 1px inset', 1)).toBeUndefined()
    expect(outerShadow('none', 1)).toBeUndefined()
  })

  it('keeps a visit once, with the title its page came to', () => {
    let history = visited([], 'https://example.com/', '', 1)
    history = visited(history, 'https://example.com/', 'Example', 2)
    expect(history).toEqual([{ url: 'https://example.com/', title: 'Example', at: 1 }])
    history = visited(history, 'https://example.com/b', 'B', 3)
    expect(history.map((visit) => visit.url)).toEqual(['https://example.com/', 'https://example.com/b'])
    // Not a web page: not kept.
    expect(visited(history, 'about:blank', '', 4)).toEqual(history)
  })

  it('finds the sites visited most, and History’s pages matching what’s typed', () => {
    const history = [
      { url: 'https://a.com/1', title: 'A one', at: 1 },
      { url: 'https://b.com/', title: 'B', at: 2 },
      { url: 'https://www.a.com/2', title: 'A two', at: 3 },
      { url: 'https://c.com/', title: 'Trucks', at: 4 },
    ]
    expect(frequentlyVisited(history, 2).map((visit) => visit.url)).toEqual(['https://www.a.com/2', 'https://c.com/'])
    expect(historyMatches(history, 'truck', 5).map((visit) => visit.url)).toEqual(['https://c.com/'])
    expect(historyMatches(history, 'a.com', 5).map((visit) => visit.url)).toEqual([
      'https://www.a.com/2',
      'https://a.com/1',
    ])
    expect(historyMatches(history, ' ', 5)).toEqual([])
  })

  it('names a tab by its page’s title, else its site', () => {
    expect(tabName({ id: 1, url: 'https://example.com/', title: 'Example Domain' })).toBe('Example Domain')
    expect(tabName({ id: 1, url: 'https://www.example.com/a', title: 'www.example.com/a' })).toBe('example.com')
    expect(tabName({ id: 1, url: '', title: '' })).toBe('')
  })

  it('types into a page’s field as keys: Backspace for what goes, the rest typed', () => {
    expect(typedSteps('hel', 'hello')).toEqual([{ action: 'type', text: 'lo' }])
    expect(typedSteps('hello', 'hell')).toEqual([{ action: 'key', key: 'backspace' }])
    // The kana keyboard turning "ka" into "か": two out, one in.
    expect(typedSteps('ka', 'か')).toEqual([
      { action: 'key', key: 'backspace' },
      { action: 'key', key: 'backspace' },
      { action: 'type', text: 'か' },
    ])
    // A character outside the basic plane is one Backspace.
    expect(typedSteps('a😀', 'a')).toEqual([{ action: 'key', key: 'backspace' }])
    expect(typedSteps('same', 'same')).toEqual([])
  })

  it('colours the bars with the page’s colour, its symbols readable on it', () => {
    expect(rgbOf('#fff')).toEqual([255, 255, 255, 1])
    expect(rgbOf('rgb(20, 30, 40)')).toEqual([20, 30, 40, 1])
    expect(rgbOf('rgba(0, 0, 0, 0)')).toEqual([0, 0, 0, 0])
    expect(rgbOf('rgb(1 2 3 / 50%)')).toEqual([1, 2, 3, 0.5])
    expect(rgbOf('red')).toBeNull()
    expect(pageColours('#ffffff')).toEqual({ bar: 'rgb(255 255 255)', look: 'light' })
    expect(pageColours('#202124')?.look).toBe('dark')
    expect(pageColours('rgba(0, 0, 0, 0)')).toBeNull()
    expect(pageColours(undefined)).toBeNull()
  })

  it('clears from where each of Safari’s timeframes starts', () => {
    const now = new Date(2026, 10, 8, 14, 30).getTime()
    expect(timeframeStart('lastHour', now)).toBe(now - 3_600_000)
    expect(timeframeStart('today', now)).toBe(new Date(2026, 10, 8).getTime())
    expect(timeframeStart('todayAndYesterday', now)).toBe(new Date(2026, 10, 7).getTime())
    expect(timeframeStart('allHistory', now)).toBe(0)
  })
})
