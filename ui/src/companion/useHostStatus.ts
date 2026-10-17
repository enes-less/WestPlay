import { call, on } from './bridge'
import { useEffect, useState } from 'react'

/** Emitted by the Tauri backend about once per second (mirrors `westplay_host::Status`). */
export interface HostStatus {
  webviewVersion: string
  adapter: string | null
  framesPerSecond: number
  droppedTotal: number
  gameConnected: boolean
  screenDrawing: boolean
  /** Whether WestPlay is open on the truck's screen (otherwise the truck's original screen shows), as the
   * cabin page last said; `null` until it has. */
  westplayOpen: boolean | null
  /** Whether YouTube is signed in (`null`: not known yet: YouTube hasn't run), and whether Google's sign-in
   * window is open. Nothing else of the account. */
  youtubeSignedIn: boolean | null
  youtubeSigningIn: boolean
  /** The same of Spotify: signed in or not (`null`: not known yet), and its sign-in window open. */
  spotifySignedIn: boolean | null
  spotifySigningIn: boolean
  /** The same of Apple Music: signed in or not (`null`: not known yet), and Apple's sign-in window open. */
  appleMusicSignedIn: boolean | null
  appleMusicSigningIn: boolean
  /** The map of the profile played (mirrors `westplay_host::maps::MapStatus`). */
  map: MapStatus
  /** The map the cab shows: its folder in WestPlay's maps, and when it was built (seconds since 1970;
   * `null`: none yet). */
  mapFolder: string | null
  mapBuilt: number | null
  error: string | null
}

/** What a map being built is doing (mirrors `westplay_host::maps::MapStage`). */
export type MapStage = 'reading' | 'drawing' | 'tiles' | 'saving'

export type MapStatus =
  | { state: 'unknown' }
  /** WestPlay isn't set up in a game yet. */
  | { state: 'noGame' }
  | { state: 'ready' }
  /** How far, 0 to 1. */
  | { state: 'building'; done: number; stage: MapStage }
  /** To build once the game is closed. */
  | { state: 'waitingForGame' }
  /** To build once the PC has `needed` bytes of memory free (it has `free` now). */
  | { state: 'waitingForMemory'; needed: number; free: number }
  /** Road to Heavy: no map for it. */
  | { state: 'notAllowed' }
  | { state: 'failed'; error: string }

/** Latest host status, or `null` until the first update arrives. */
export function useHostStatus(): HostStatus | null {
  const [status, setStatus] = useState<HostStatus | null>(null)
  useEffect(() => {
    return on<HostStatus>('host-status', setStatus)
  }, [])
  return status
}

/** Opens Google's sign-in for YouTube in a window on the desktop. */
export function youtubeSignIn(): Promise<void> {
  return call('youtube_sign_in')
}

/** Signs YouTube out. */
export function youtubeSignOut(): Promise<void> {
  return call('youtube_sign_out')
}

/** Runs YouTube (hidden), so the status says whether it's signed in. */
export function youtubeCheck(): Promise<void> {
  return call('youtube_check')
}

/** Opens Spotify's sign-in in a window on the desktop. */
export function spotifySignIn(): Promise<void> {
  return call('spotify_sign_in')
}

/** Signs Spotify out. */
export function spotifySignOut(): Promise<void> {
  return call('spotify_sign_out')
}

/** Runs Spotify's web player (hidden), so the status says whether it's signed in. */
export function spotifyCheck(): Promise<void> {
  return call('spotify_check')
}

/** Opens Apple's sign-in for Apple Music in a window on the desktop. */
export function appleMusicSignIn(): Promise<void> {
  return call('apple_music_sign_in')
}

/** Signs Apple Music out. */
export function appleMusicSignOut(): Promise<void> {
  return call('apple_music_sign_out')
}

/** Opens the support page (Kreosus) in the player's browser. */
export function openSupportPage(): Promise<void> {
  return call('open_support_page')
}

/** Runs Apple Music's web player (hidden), so the status says whether it's signed in. */
export function appleMusicCheck(): Promise<void> {
  return call('apple_music_check')
}

/** Opens or closes WestPlay on the truck's screen (the status then reports it, once the cabin has). */
export function setWestplayOpen(open: boolean): Promise<void> {
  return call('set_westplay_open', { open })
}
