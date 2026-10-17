import { describe, expect, it } from 'vitest'
import { solidLuminance } from './cursorLight'

describe('the cursor over light things', () => {
  it('reads a solid background, light or dark', () => {
    expect(solidLuminance('rgb(242, 242, 246)')).toBeGreaterThan(0.8)
    expect(solidLuminance('rgb(11, 11, 13)')).toBeLessThan(0.01)
  })

  it('looks through what is mostly see-through', () => {
    expect(solidLuminance('rgba(0, 0, 0, 0)')).toBeNull()
    expect(solidLuminance('transparent')).toBeNull()
    expect(solidLuminance('rgba(255, 255, 255, 0.12)')).toBeNull()
    expect(solidLuminance('rgba(255, 255, 255, 0.9)')).toBeGreaterThan(0.8)
  })
})
