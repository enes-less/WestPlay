import { useEffect, useState } from 'react'
import type { MediaSource } from '../../../shared/media'
import { type ChangeDirection, MAX_COVER_WAIT_MS, readyToShow, TRACK_CHANGE_MS, trackKey } from './track'

/** A track as the screen shows it. */
export interface ShownTrack {
  key: string
  /** The latest report for this track (position, play state). */
  source: MediaSource
  /** Its cover as shown; while the next track's cover is late, the previous one stays. */
  artwork: string | null
}

export interface TrackTransition {
  /** `null`: Not Playing. */
  shown: ShownTrack | null
  /** What slides out while a change animates (its `track` is `null` when that was Not Playing). */
  leaving: { track: ShownTrack | null } | null
  direction: ChangeDirection
}

/**
 * The track Music shows. A new track replaces it only once its cover has arrived (or it has none, or
 * `MAX_COVER_WAIT_MS` passed), so text, cover and background change together (track.ts).
 */
export function useTrackTransition(source: MediaSource | null, artwork: string | null): TrackTransition {
  const [shown, setShown] = useState<ShownTrack | null>(() =>
    source ? { key: trackKey(source), source, artwork } : null,
  )
  const [leaving, setLeaving] = useState<{ track: ShownTrack | null } | null>(null)
  const [direction, setDirection] = useState<ChangeDirection>('forward')
  const [waitOver, setWaitOver] = useState<string | null>(null)

  const incomingKey = source ? trackKey(source) : null
  const shownKey = shown?.key ?? null

  // Adjusting state during render: the shown track follows the incoming one by these rules.
  if (source && incomingKey === shownKey && shown) {
    // The same track: newer position or play state, or its cover arriving (or changing) later.
    const nextArtwork = artwork ?? shown.artwork
    if (source !== shown.source || nextArtwork !== shown.artwork) {
      setShown({ key: shown.key, source, artwork: nextArtwork })
    }
  } else if (!source) {
    if (shown) {
      setLeaving({ track: shown })
      setDirection('forward')
      setShown(null)
    }
  } else if (incomingKey && readyToShow(source, artwork, waitOver === incomingKey)) {
    // A new track, ready: everything changes at once. A late cover keeps the old one until it arrives.
    const sameApp = shown?.source.id === source.id
    setLeaving({ track: shown })
    setDirection('forward')
    setShown({ key: incomingKey, source, artwork: artwork ?? (sameApp ? (shown?.artwork ?? null) : null) })
  }

  // A new track's cover is waited for only so long.
  useEffect(() => {
    if (!incomingKey || incomingKey === shownKey) return
    const timer = setTimeout(() => setWaitOver(incomingKey), MAX_COVER_WAIT_MS)
    return () => clearTimeout(timer)
  }, [incomingKey, shownKey])

  // The leaving track is gone once its animation is over.
  useEffect(() => {
    if (!leaving) return
    const timer = setTimeout(() => setLeaving(null), TRACK_CHANGE_MS)
    return () => clearTimeout(timer)
  }, [leaving])

  return { shown, leaving, direction }
}
