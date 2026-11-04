import { useMemo } from 'react'
import { create } from 'zustand'
import {
  isInHost,
  type PlayerCommand,
  type PlayerPlayback,
  type PlayerRepeat,
  type PlayerSong,
  postToHost,
} from '../../../shared/host'
import type { MediaSource } from '../../../shared/media'
import { SONGS } from './mock'

/**
 * Player: WestPlay's own player for the game's music folder (`westplay_host::music_folder`). The host lists the
 * folder's songs (`musicFolder`) and plays them in a player of its own, its sound through the truck's speakers;
 * this follows what it says plays (`playerPlayback`) and asks it to play. In a plain browser (no host), made-up
 * songs stand in for the folder (mock.ts), and nothing plays.
 */
export interface PlayerState {
  /** The folder's songs, in its order; `null` until the host has said them. */
  songs: PlayerSong[] | null
  /** Where the folder is (`null`: not known). */
  folder: string | null
  /** What plays: the host's, its position at `at` (performance.now ms); `null`: nothing. */
  playback: (PlayerPlayback & { at: number }) | null
  /** Now Playing open over the app. */
  nowPlaying: boolean
  /** The volume the player chose (0 to 1): a fade's steps aren't it. */
  volume: number
}

/** In the host (not the cab's preview in a plain browser, nor a test without a page). */
const IN_HOST = typeof window !== 'undefined' && isInHost()

export const usePlayer = create<PlayerState>(() => ({
  songs: IN_HOST ? null : SONGS,
  folder: IN_HOST ? null : 'Documents\\Euro Truck Simulator 2\\music',
  playback: null,
  nowPlaying: false,
  volume: 1,
}))

/** Where the songs' covers are served (the host's `music_folder::COVERS_HOST`). */
const COVERS = 'https://westplay.music-covers/'

/** A song's cover, or `null` without one. */
export function coverOf(song: PlayerSong | null | undefined): string | null {
  return song?.cover ? COVERS + encodeURIComponent(song.cover) : null
}

/** A song's name: its title, else its file's name without its extension. */
export function nameOf(song: PlayerSong): string {
  return song.title ?? song.file.replace(/\.[^.]+$/, '')
}

function send(command: PlayerCommand): void {
  if (isInHost()) postToHost({ type: 'player', command })
}

/** The folder as the host listed it. */
export function receiveFolder(folder: string | null, songs: PlayerSong[]): void {
  usePlayer.setState({ folder, songs })
}

/** What the host says plays. */
export function receivePlayback(playback: PlayerPlayback | null): void {
  usePlayer.setState({
    playback: playback ? { ...playback, at: performance.now() } : null,
    ...(playback ? {} : { nowPlaying: false }),
  })
}

/** The song playing (as the folder lists it), or `null`. */
export function currentSong(state: Pick<PlayerState, 'songs' | 'playback'>): PlayerSong | null {
  const file = state.playback?.file
  return (file && state.songs?.find((song) => song.file === file)) || (file ? { file, seconds: 0 } : null)
}

/** The song playing (`currentSong`), as a hook: the same object while it's the same song. */
export function useCurrentSong(): PlayerSong | null {
  const songs = usePlayer((state) => state.songs)
  const file = usePlayer((state) => state.playback?.file ?? null)
  return useMemo(() => (file ? currentSong({ songs, playback: { file } as PlayerState['playback'] }) : null), [songs, file])
}

/** Where the song is now (seconds): the time runs on from what the host said while it plays. */
export function positionOf(playback: PlayerState['playback'], now: number): number {
  if (!playback) return 0
  const run = playback.playing && !playback.loading ? (now - playback.at) / 1000 : 0
  const end = playback.duration || Number.POSITIVE_INFINITY
  return Math.min(end, playback.position + run)
}

/** Shows a change at once (the host's next word confirms it). */
function show(change: Partial<PlayerPlayback>): void {
  const { playback } = usePlayer.getState()
  if (!playback) return
  const now = performance.now()
  usePlayer.setState({ playback: { ...playback, position: positionOf(playback, now), at: now, ...change } })
}

/** Plays the folder's songs from the one at `index` (shuffled: from any, `index` none). */
export function play(index: number | undefined, shuffle: boolean): void {
  const songs = usePlayer.getState().songs ?? []
  if (songs.length === 0) return
  // At the volume chosen (a player started anew plays at full).
  sendVolume(usePlayer.getState().volume)
  send({ action: 'play', files: songs.map((song) => song.file), index, shuffle })
}

export function playOrPause(): void {
  const { playback } = usePlayer.getState()
  if (!playback) return
  if (playback.playing) {
    send({ action: 'pause' })
    show({ playing: false, loading: false })
  } else {
    send({ action: 'play' })
    show({ playing: true })
  }
}

/** Next (1) or Back (-1): Back past a song's first seconds is the song's start. */
export function skip(direction: 1 | -1): void {
  if (!usePlayer.getState().playback) return
  send({ action: direction === 1 ? 'next' : 'previous' })
}

export function seek(seconds: number): void {
  send({ action: 'seek', seconds })
  show({ position: Math.max(0, seconds) })
}

export function setShuffle(on: boolean): void {
  send({ action: 'shuffle', on })
  show({ shuffle: on })
}

export function cycleRepeat(): void {
  const repeat = usePlayer.getState().playback?.repeat ?? 'off'
  const next: PlayerRepeat = repeat === 'off' ? 'all' : repeat === 'all' ? 'one' : 'off'
  send({ action: 'repeat', mode: next })
  show({ repeat: next })
}

/** Sets the volume it plays at (0 to 1) without making it the player's choice: a fade's step. */
export function sendVolume(share: number): void {
  send({ action: 'volume', percent: Math.round(Math.min(1, Math.max(0, share)) * 100) })
}

/** Sets the volume it plays at (0 to 1) as the player's choice (a keybind). */
export function setVolume(share: number): void {
  const volume = Math.min(1, Math.max(0, share))
  usePlayer.setState({ volume })
  sendVolume(volume)
}

/** Whether Player plays now. */
export function playingNow(): boolean {
  return Boolean(usePlayer.getState().playback?.playing)
}

/** Pauses what plays (the game paused, tabbed out of, its electrics off; another player started). */
export function pauseNow(): void {
  if (!playingNow()) return
  send({ action: 'pause' })
  show({ playing: false, loading: false })
}

/** Plays on what was paused (back in the game). */
export function resumeNow(): void {
  const { playback } = usePlayer.getState()
  if (!playback || playback.playing) return
  send({ action: 'play' })
  show({ playing: true })
}

/** What Player plays, as the Dashboard's Now Playing takes music, and its cover; `null` while nothing does. */
export function usePlayerSource(): { source: MediaSource; artwork: string | null } | null {
  const playback = usePlayer((state) => state.playback)
  const songs = usePlayer((state) => state.songs)
  return useMemo(() => {
    const song = currentSong({ songs, playback })
    if (!playback || !song) return null
    return {
      source: {
        id: 'westplay-player',
        service: 'player',
        title: nameOf(song),
        artist: song.artist ?? '',
        album: '',
        artwork: null,
        status: playback.playing ? 'playing' : 'paused',
        positionMs: playback.position * 1000,
        positionAt: Math.round(performance.timeOrigin + playback.at),
        durationMs: (playback.duration || song.seconds) * 1000,
        controls: { playPause: true, next: true, previous: true, shuffle: true, repeat: true },
        shuffle: playback.shuffle,
        repeat: playback.repeat === 'one' ? 'track' : playback.repeat === 'all' ? 'list' : 'off',
      },
      artwork: coverOf(song),
    }
  }, [playback, songs])
}
