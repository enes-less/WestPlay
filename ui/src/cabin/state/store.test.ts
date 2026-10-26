import { afterEach, describe, expect, it, vi } from 'vitest'

/** A browser's storage holding what an earlier WestPlay saved (and the page around it, outside WestPlay). */
function savedStorage(state: unknown, version = 1) {
  vi.stubGlobal('window', {})
  vi.stubGlobal('location', { search: '' })
  const items = new Map([['westplay.cabin', JSON.stringify({ state, version })]])
  return {
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
    removeItem: (key: string) => void items.delete(key),
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.resetModules()
})

describe('the saved Settings choices', () => {
  it('come back as they were', async () => {
    vi.stubGlobal(
      'localStorage',
      savedStorage({ dashboardWidgets: { nowPlaying: false, destination: true, weather: false }, albumArt: false }),
    )
    const { useCabin } = await import('./store')
    expect(useCabin.getState().dashboardWidgets).toEqual({ nowPlaying: false, destination: true, weather: false })
    expect(useCabin.getState().albumArt).toBe(false)
  })

  it('keep a widget shown unless it was turned off, and ignore what isn’t a choice', async () => {
    vi.stubGlobal(
      'localStorage',
      savedStorage({ dashboardWidgets: { weather: false, nowPlaying: 'no' }, albumArt: 'off' }),
    )
    const { useCabin } = await import('./store')
    expect(useCabin.getState().dashboardWidgets).toEqual({ nowPlaying: true, destination: true, weather: false })
    expect(useCabin.getState().albumArt).toBe(true)
  })

  it('are all on when nothing was saved (an earlier WestPlay)', async () => {
    vi.stubGlobal('localStorage', savedStorage({ appearanceMode: 'dark' }))
    const { useCabin } = await import('./store')
    expect(useCabin.getState().dashboardWidgets).toEqual({ nowPlaying: true, destination: true, weather: true })
    expect(useCabin.getState().albumArt).toBe(true)
    expect(useCabin.getState().appearanceMode).toBe('dark')
  })
})

describe('the saved Text Size', () => {
  it('stays the size it was when smaller steps are added below it', async () => {
    // 1.7, the sixth step of seven before 0.8 and 0.9 were added.
    vi.stubGlobal('localStorage', savedStorage({ textSize: 5 }, 2))
    const { useCabin } = await import('./store')
    const { textScale } = await import('./textSize')
    expect(textScale(useCabin.getState().textSize)).toBe(1.7)
  })

  it('starts on the default when it was kept from before the steps moved larger', async () => {
    vi.stubGlobal('localStorage', savedStorage({ textSize: 5 }, 1))
    const { useCabin } = await import('./store')
    const { DEFAULT_TEXT_SIZE } = await import('./textSize')
    expect(useCabin.getState().textSize).toBe(DEFAULT_TEXT_SIZE)
  })
})
