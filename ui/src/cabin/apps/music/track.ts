import type { MediaSource } from '../../../shared/media'

/**
 * When the screen (Music, the Dashboard's Now Playing) changes from one track to the next. Players announce a new track a moment before its cover
 * arrives, so the screen waits for the cover (at most `MAX_COVER_WAIT_MS`) and then changes everything at
 * once: the text slides, the cover and the background crossfade.
 */

/** The longest the screen waits for a new track's cover before changing anyway. */
export const MAX_COVER_WAIT_MS = 1200

/** How long the change takes; must match --track-change in trackChange.css. */
export const TRACK_CHANGE_MS = 480

export type ChangeDirection = 'forward' | 'backward'

/** Identifies a track: a different key is a different track (the same song from another app included). */
export function trackKey(source: MediaSource): string {
  return [source.id, source.title, source.artist, source.album].join('\n')
}

/**
 * Whether a new track can be shown: its cover has arrived (`artwork`), it has none, or the wait is over.
 */
export function readyToShow(incoming: MediaSource, artwork: string | null, waitOver: boolean): boolean {
  return incoming.artwork === null || artwork !== null || waitOver
}
