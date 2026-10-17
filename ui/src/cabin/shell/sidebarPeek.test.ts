import { describe, expect, it } from 'vitest'
import { sidebarPeeks } from './sidebarPeek'

// The CarPlay area beside a 96 px strip, as on the Scania Smart Dash; the sidebar out ends at 246.
const area = { left: 96, top: 0, width: 1184, height: 640 }
const out = 246

describe('sidebarPeeks', () => {
  it('comes out at the left edge, near the middle', () => {
    expect(sidebarPeeks(false, { x: 100, y: 320 }, area, -200)).toBe(true)
    expect(sidebarPeeks(false, { x: 96 + 70, y: 200 }, area, -200)).toBe(true)
  })

  it('not at the edge’s ends (the player’s corner buttons), nor further in', () => {
    expect(sidebarPeeks(false, { x: 100, y: 60 }, area, -200)).toBe(false)
    expect(sidebarPeeks(false, { x: 100, y: 600 }, area, -200)).toBe(false)
    expect(sidebarPeeks(false, { x: 96 + 90, y: 320 }, area, -200)).toBe(false)
    // Over the strip beside the CarPlay area.
    expect(sidebarPeeks(false, { x: 40, y: 320 }, area, -200)).toBe(false)
  })

  it('stays while the pointer is on it or beside it, anywhere along it', () => {
    expect(sidebarPeeks(true, { x: 200, y: 600 }, area, out)).toBe(true)
    expect(sidebarPeeks(true, { x: out + 40, y: 80 }, area, out)).toBe(true)
  })

  it('goes once the pointer moves off, or leaves the screen', () => {
    expect(sidebarPeeks(true, { x: out + 60, y: 320 }, area, out)).toBe(false)
    expect(sidebarPeeks(true, null, area, out)).toBe(false)
  })
})
