import { describe, expect, it } from 'vitest'
import { follow, LOAD_MS } from './follow'

describe('follow', () => {
  it('brings back the video YouTube plays after the page reloaded', () => {
    expect(follow(null, 'abc', null, 0)).toEqual({ shown: 'restore', loading: null })
    expect(follow('abc', 'abc', null, 0)).toEqual({ shown: 'keep', loading: null })
  })

  it('keeps a video asked for while YouTube still reports the one before', () => {
    const loading = { video: 'new', at: 1000 }
    expect(follow('new', 'old', loading, 2000)).toEqual({ shown: 'keep', loading })
    // Until it reports it,
    expect(follow('new', 'new', loading, 3000)).toEqual({ shown: 'keep', loading: null })
    // or takes too long: then what it plays is what's shown.
    expect(follow('new', 'old', loading, 1000 + LOAD_MS)).toEqual({ shown: 'restore', loading: null })
  })

  it('shows nothing playing when YouTube isn’t running, unless one is on its way', () => {
    expect(follow('abc', null, null, 0)).toEqual({ shown: 'clear', loading: null })
    expect(follow(null, null, null, 0)).toEqual({ shown: 'keep', loading: null })
    const loading = { video: 'abc', at: 0 }
    expect(follow('abc', null, loading, 500)).toEqual({ shown: 'keep', loading })
  })
})
