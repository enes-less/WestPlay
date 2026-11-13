import { describe, expect, it } from 'vitest'
import { gameStoppedMap, nextSheet, type StoppedSheet } from './gameStopped'
import type { HostStatus, MapStatus, ProfileMap } from './useHostStatus'

const status = (map: MapStatus, profiles: MapStatus[] = []): HostStatus =>
  ({
    map,
    profileMaps: profiles.map((status, at): ProfileMap => ({
      game: 'ets2',
      dir: `profile${at}`,
      folder: `map${at}`,
      status,
    })),
  }) as HostStatus

describe('a map stopped by the game', () => {
  it('is any profile’s map stopped, not one only waiting', () => {
    expect(gameStoppedMap(null)).toBe(false)
    expect(gameStoppedMap(status({ state: 'waitingForGame' }))).toBe(false)
    expect(gameStoppedMap(status({ state: 'waitingForGame', stopped: true }))).toBe(true)
    expect(gameStoppedMap(status({ state: 'ready' }, [{ state: 'ready' }, { state: 'waitingForGame' }]))).toBe(false)
    expect(gameStoppedMap(status({ state: 'ready' }, [{ state: 'waitingForGame', stopped: true }]))).toBe(true)
  })

  it('puts the sheet up once each time, kept until it’s closed', () => {
    let sheet: StoppedSheet = { shown: false, was: false }
    const look = (stopped: boolean) => (sheet = nextSheet(sheet, stopped))
    expect(look(false).shown).toBe(false)
    expect(look(true).shown).toBe(true)
    // The game closed, the map preparing again before the player looked: still up.
    expect(look(false).shown).toBe(true)
    sheet = { ...sheet, shown: false }
    expect(look(false).shown).toBe(false)
    // Stopped again: up again; closed while still stopped, it stays down.
    expect(look(true).shown).toBe(true)
    sheet = { ...sheet, shown: false }
    expect(look(true).shown).toBe(false)
    // The same sheet when nothing changed (React's render isn't asked again).
    expect(nextSheet(sheet, true)).toBe(sheet)
  })
})
