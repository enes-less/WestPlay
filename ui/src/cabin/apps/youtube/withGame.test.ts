import { describe, expect, it } from 'vitest'
import { poweredOff, withGame } from './withGame'

describe('withGame', () => {
  it('pauses a video playing as the player goes away from the game', () => {
    expect(withGame(true, false, true, false, true)).toBe('pause')
    // Nothing playing: nothing to pause.
    expect(withGame(true, false, false, false, true)).toBe(null)
  })

  it('plays on as they come back, only what it paused', () => {
    expect(withGame(false, true, false, true, true)).toBe('play')
    // Paused by the player: stays paused.
    expect(withGame(false, true, false, false, true)).toBe(null)
  })

  it('plays nothing on with the electrics off', () => {
    expect(withGame(false, true, false, true, false)).toBe(null)
    // Pausing still pauses.
    expect(withGame(true, false, true, false, false)).toBe('pause')
  })

  it('does nothing while nothing changes', () => {
    expect(withGame(true, true, true, true, true)).toBe(null)
    expect(withGame(false, false, true, false, true)).toBe(null)
  })
})

describe('poweredOff', () => {
  it('is the moment the electrics go off, not while they stay off or come on', () => {
    expect(poweredOff(false, true)).toBe(true)
    expect(poweredOff(true, true)).toBe(false)
    expect(poweredOff(true, false)).toBe(false)
  })
})
