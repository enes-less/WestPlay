import { describe, expect, it } from 'vitest'
import {
  BUILT_IN_WALLPAPERS,
  customWallpaper,
  DEFAULT_WALLPAPER_ID,
  findWallpaper,
  pickVariant,
  type Wallpaper,
} from './wallpapers'

const both: Wallpaper = { id: 'both', name: 'Both', variants: { dark: 'd.jpg', light: 'l.jpg' } }
const darkOnly: Wallpaper = { id: 'dark', name: 'Dark', variants: { dark: 'd.svg' } }

describe('pickVariant', () => {
  it('uses the matching variant', () => {
    expect(pickVariant(both, 'light')).toEqual({ appearance: 'light', src: 'l.jpg' })
    expect(pickVariant(both, 'dark')).toEqual({ appearance: 'dark', src: 'd.jpg' })
  })

  it('falls back to the other variant and makes the UI follow it', () => {
    expect(pickVariant(darkOnly, 'light')).toEqual({ appearance: 'dark', src: 'd.svg' })
  })
})

describe('registry', () => {
  it('has unique ids and at least one variant per wallpaper', () => {
    expect(new Set(BUILT_IN_WALLPAPERS.map((w) => w.id)).size).toBe(BUILT_IN_WALLPAPERS.length)
    for (const wallpaper of BUILT_IN_WALLPAPERS) expect(Object.keys(wallpaper.variants).length).toBeGreaterThan(0)
  })

  it('falls back to the default for unknown ids (a wallpaper the player removed)', () => {
    expect(findWallpaper('missing', BUILT_IN_WALLPAPERS).id).toBe(DEFAULT_WALLPAPER_ID)
    expect(findWallpaper('custom:gone', [both]).id).toBe(DEFAULT_WALLPAPER_ID)
  })
})

describe('the player’s wallpapers', () => {
  it('take their files from where they’re served, each part of the path encoded', () => {
    const custom = customWallpaper(
      { id: 'custom:Dusk', name: 'Dusk', dark: 'wallpapers/Dusk (2).dark.jpg', light: null },
      'https://westplay.customize/',
    )
    expect(custom).toEqual({
      id: 'custom:Dusk',
      name: 'Dusk',
      variants: { dark: 'https://westplay.customize/wallpapers/Dusk%20(2).dark.jpg' },
    })
    expect(findWallpaper('custom:Dusk', [...BUILT_IN_WALLPAPERS, custom])).toBe(custom)
  })

  it('one picture for both looks leaves Light and Dark to the player', () => {
    const custom = customWallpaper(
      { id: 'custom:Night.png', name: 'Night', dark: 'wallpapers/Night.png', light: 'wallpapers/Night.png' },
      'https://westplay.customize/',
    )
    const src = 'https://westplay.customize/wallpapers/Night.png'
    expect(pickVariant(custom, 'light')).toEqual({ appearance: 'light', src })
    expect(pickVariant(custom, 'dark')).toEqual({ appearance: 'dark', src })
  })
})
