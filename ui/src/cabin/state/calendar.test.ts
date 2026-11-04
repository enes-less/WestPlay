import { describe, expect, it } from 'vitest'
import { calendarIconSvg, calendarWeekday } from './calendar'

describe('the Calendar', () => {
  it("takes the game's weekday, the PC's while no game runs", () => {
    // 3 November 2026 is a Tuesday on the PC.
    const pc = new Date(2026, 10, 3)
    expect(calendarWeekday(null, pc)).toBe(1)
    // The game's day 0 is a Monday: day 12 a Saturday, whatever the PC says.
    expect(calendarWeekday(12, pc)).toBe(5)
    expect(calendarWeekday(0, pc)).toBe(0)
  })

  it('writes the weekday over the day on its card, the weekday in red', () => {
    const svg = calendarIconSvg('<svg><path/>\n  <!-- date -->\n</svg>', 'light', 'Pzt', 2)
    expect(svg).not.toContain('<!-- date -->')
    expect(svg).toMatch(/fill="rgb\(255 56 60\)">Pzt<\/text>/)
    expect(svg).toMatch(/>2<\/text>/)
    expect(calendarIconSvg('<!-- date -->', 'dark', 'A<B', 9)).toContain('A&lt;B')
  })
})
