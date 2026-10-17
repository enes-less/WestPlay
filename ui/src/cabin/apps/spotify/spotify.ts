import { useEffect, useMemo, useState } from 'react'
import { create } from 'zustand'
import { createAsking } from '../asking'
import {
  isInHost,
  postToHost,
  type SpotifyAnswers,
  type SpotifyCommand,
  type SpotifyItem,
  type SpotifyPlayback,
  type SpotifyQuestion,
  type SpotifyRepeat,
  type SpotifyTrack,
} from '../../../shared/host'
import type { MediaSource } from '../../../shared/media'
import { forgetKept, keep } from './kept'
import { previewAnswer } from './preview'

export type {
  SpotifyArtist,
  SpotifyArtistName,
  SpotifyCategory,
  SpotifyCollection,
  SpotifyHome,
  SpotifyItem,
  SpotifyResults,
  SpotifyTrack,
} from '../../../shared/host'

/**
 * The Spotify app's state. The app is WestPlay's own, drawn in Spotify's look; Spotify's web player, run by
 * the host in a hidden WebView (crates/host spotify.rs), plays and answers the app's questions (Home, Your
 * Library, search, a playlist's songs) from its own page, as YouTube's does. The app shows what it asked for
 * at once (a song pressed plays here before Spotify says so) and then follows what Spotify says plays, here
 * or on another of the account's devices (Spotify Connect). In a plain browser an invented preview answers
 * and plays (preview.ts).
 *
 * Signed out, the app is Spotify's preview, as its web player is: its public Home and pages, playing or
 * saving asking to log in first. Signing in (or up) is Spotify's own page in a window on the PC's desktop.
 * The host says whether it's signed in, nothing more of the account. Nothing of the account is kept:
 * answers are only shown (and held for this session, to go back to).
 */

export type Question = SpotifyQuestion
export type Answers = SpotifyAnswers
export type Repeat = SpotifyRepeat

export type Tab = 'home' | 'search' | 'library'

/** A page over a tab's own. */
export type View =
  | { id: number; kind: 'collection'; uri: string }
  | { id: number; kind: 'artist'; uri: string }
  | { id: number; kind: 'results'; query: string }
  /** A shelf's Show all: every card of it. */
  | { id: number; kind: 'shelf'; title: string; items: SpotifyItem[] }

/** Where what plays plays from. */
export interface PlayContext {
  uri: string
  name: string
  kind: 'playlist' | 'album' | 'artist' | 'show' | 'liked'
}

/** What plays: the song (first of `tracks`, or `index`), those after it, where it plays from (`null`: not
 * known, it was started elsewhere), and how far. */
export interface Playback {
  context: PlayContext | null
  tracks: SpotifyTrack[]
  index: number
  playing: boolean
  /** Seconds into the song at `at` (`performance.now()`), running on from there while it plays. */
  position: number
  at: number
  shuffle: boolean
  repeat: Repeat
  /** The playing device's volume, 0 to 1 (`null`: none Spotify can set). */
  volume: number | null
  /** Playing in WestPlay itself, not on another of the account's devices (Spotify Connect). */
  here: boolean
  /** The way the last change of song went (Now Playing's cover slides that way). */
  direction: 1 | -1
}

/** Why the log-in prompt shows: something to play (its cover beside it), to save, a playlist to make. */
export interface Prompt {
  reason: 'play' | 'save' | 'playlist'
  cover: Pick<SpotifyItem, 'name' | 'kind' | 'image' | 'color'> | null
}

interface SpotifyState {
  /** Whether Spotify's web player is signed in (the host says): `null` until it knows. */
  signedIn: boolean | null
  playing: boolean | null
  signingIn: boolean

  tab: Tab
  /** Each tab's pages over its own, the one showing last (Spotify keeps every tab where it was left). */
  views: Record<Tab, View[]>
  /** Now Playing is open, over the app. */
  nowPlaying: boolean
  /** Now Playing shows the queue beside the cover. */
  queue: boolean
  /** Now Playing shows the song's words in the cover's place. */
  lyrics: boolean
  playback: Playback | null
  /** Liked Songs and Your Library as changed in this session, by address (Spotify's own answers say the
   * rest: `saved`). */
  saved: Record<string, boolean>
  /** "Added to Liked Songs.": the toast showing. */
  toast: { text: string; id: number } | null
  /** Searches' picks, newest first: Recent searches. */
  recent: SpotifyItem[]
  /** Counted up a moment after the music's source changes (a playlist, an album, a song started): Home's
   * Recently Played is asked again then (Spotify has counted the play by then). */
  played: number
  /** Spotify's "Start listening with a free Spotify account", over the app. */
  prompt: Prompt | null
  /** Plays Spotify didn't confirm, counted (the app says it couldn't play, each time). */
  unplayed: number
}

export const useSpotify = create<SpotifyState>()(() => ({
  // Outside WestPlay (a plain browser) there's no host to say: the preview starts signed out.
  signedIn: isInHost() ? null : false,
  playing: null,
  signingIn: false,
  tab: 'home',
  views: { home: [], search: [], library: [] },
  nowPlaying: false,
  queue: false,
  lyrics: false,
  playback: null,
  saved: {},
  toast: null,
  recent: [],
  played: 0,
  prompt: null,
  unplayed: 0,
}))

export function spotify(command: SpotifyCommand): void {
  postToHost({ type: 'spotify', command })
}

// --- The account -------------------------------------------------------------------------------------------

/** How long the app waits for the web player to say before showing Spotify signed out. */
const ACCOUNT_WAIT = 8000

/** Signed in, signed out, or not known yet (the web player starting): known by `ACCOUNT_WAIT` at the latest,
 * signed out then (a web player that didn't load can't play anyway). */
export function useAccount(): 'in' | 'out' | 'unknown' {
  const signedIn = useSpotify((state) => state.signedIn)
  const [waited, setWaited] = useState(false)
  useEffect(() => {
    if (signedIn !== null) return
    const timer = setTimeout(() => setWaited(true), ACCOUNT_WAIT)
    return () => clearTimeout(timer)
  }, [signedIn])
  return signedIn === null ? (waited ? 'out' : 'unknown') : signedIn ? 'in' : 'out'
}

/** The preview's sign-in: as if done on the PC a moment later. */
const PREVIEW_SIGN_IN = 1800

function openSignIn(action: 'signIn' | 'signUp'): void {
  useSpotify.setState({ prompt: null })
  if (isInHost()) {
    spotify({ action })
    return
  }
  useSpotify.setState({ signingIn: true })
  setTimeout(() => useSpotify.setState({ signingIn: false, signedIn: true }), PREVIEW_SIGN_IN)
}

/** Spotify's log-in, in a window on the PC's desktop. */
export function logIn(): void {
  openSignIn('signIn')
}

/** Spotify's sign-up, in that window: once the account's made, it's signed in. */
export function signUp(): void {
  openSignIn('signUp')
}

export function logOut(): void {
  if (isInHost()) spotify({ action: 'signOut' })
  else useSpotify.setState({ signedIn: false })
}

/** Signed in, or the log-in prompt shows (why, and what was to play): whether to go on. */
function signedInOr(prompt: Prompt): boolean {
  if (useSpotify.getState().signedIn) return true
  useSpotify.setState({ prompt })
  return false
}

// Signed in or out: nothing's kept of what the account had (its answers asked again, for the other).
useSpotify.subscribe((state, before) => {
  // Signed out (or found signed out as Spotify starts): what was kept of the account goes.
  if (state.signedIn === false && before.signedIn !== false) forgetKept()
  if (state.signedIn !== before.signedIn && before.signedIn !== null) {
    asking.clear()
    useSpotify.setState({
      playback: state.signedIn ? state.playback : null,
      nowPlaying: false,
      saved: {},
      recent: [],
      views: { home: [], search: [], library: [] },
    })
  }
})

// --- Questions ---------------------------------------------------------------------------------------------

/** Home's Recently Played (all of Home's, signed in) and Your Library's list, kept to show at once next time
 * (kept.ts). */
function keepAnswer(question: Question, answer: unknown): void {
  if (question.ask === 'home' && question.signedIn && !question.facet) {
    const home = answer as Answers['home']
    const recent = home.sections.find((section) => section.recent)
    keep({ shortcuts: home.shortcuts, recent: recent ? { title: recent.title, items: recent.items } : null })
  } else if (question.ask === 'library') {
    keep({ library: answer as Answers['library'] })
  }
}

const asking = createAsking<Question, Answers>(
  (id, question) => spotify({ action: 'ask', id, question }),
  previewAnswer,
  isInHost,
  keepAnswer,
)
/** A question's answer: as known this session, or (`fresh`, or not known) asked of Spotify. */
export const ask = asking.ask
/** A question's answer once it comes (`null` until then), whether Spotify failed to answer, and asking
 * again; asked again when the question changes. A page whose answer changes as Spotify is used (Home's Recently
 * Played, Your Library) gives a `refresh`: it's asked afresh when that changes and whenever the answer is known
 * already (the page opened anew), the known answer shown meanwhile. */
export const useAsked = asking.useAsked
/** Spotify's answer (`spotifyAnswer`), for the question asked with its id. */
export const receiveAnswer = asking.receive

export function useAnswer<Q extends Question>(question: Q | null): Answers[Q['ask']] | null {
  return useAsked(question).answer
}

// --- Going around ------------------------------------------------------------------------------------------

let nextId = 1

/** A tab: pressed again where it already is, back to its own page (Spotify's way). */
export function openTab(tab: Tab): void {
  const { tab: was, views } = useSpotify.getState()
  if (was === tab) useSpotify.setState({ views: { ...views, [tab]: [] } })
  else useSpotify.setState({ tab })
}

type Unidentified<T> = T extends unknown ? Omit<T, 'id'> : never

export function openView(view: Unidentified<View>): void {
  const { tab, views } = useSpotify.getState()
  useSpotify.setState({ views: { ...views, [tab]: [...views[tab], { ...view, id: nextId++ } as View] } })
}

/** A card or row pressed: its page; a song, it plays (from its album). */
export function openItem(item: SpotifyItem): void {
  if (item.kind === 'track') playSong(item)
  else if (item.kind === 'artist') openView({ kind: 'artist', uri: item.uri })
  else openView({ kind: 'collection', uri: item.uri })
}

export function goBack(): void {
  const { tab, views } = useSpotify.getState()
  useSpotify.setState({ views: { ...views, [tab]: views[tab].slice(0, -1) } })
}

export function remember(item: SpotifyItem): void {
  const recent = useSpotify.getState().recent.filter((r) => r.uri !== item.uri)
  useSpotify.setState({ recent: [item, ...recent].slice(0, 10) })
}

export function forget(uri: string): void {
  useSpotify.setState({ recent: useSpotify.getState().recent.filter((r) => r.uri !== uri) })
}

// --- Playing -----------------------------------------------------------------------------------------------

/** Seconds into the song now. */
export function positionOf(playback: Playback, now = performance.now()): number {
  // `now` may be a clock's last tick from before the song's own time (played on after a pause): never before
  // it, as the shared media clock has it.
  const position = playback.playing ? playback.position + Math.max(0, now - playback.at) / 1000 : playback.position
  return Math.min(position, playback.tracks[playback.index]?.duration ?? 0)
}

export function currentTrack(playback: Playback | null): SpotifyTrack | null {
  return playback ? (playback.tracks[playback.index] ?? null) : null
}

function contextOf(item: SpotifyItem): PlayContext {
  return { uri: item.uri, name: item.name, kind: item.kind === 'track' ? 'album' : item.kind }
}

/** A change of what plays, shown at once (Spotify then says how it really is). */
function showPlayback(change: Partial<Playback>): void {
  const playback = useSpotify.getState().playback
  if (playback) useSpotify.setState({ playback: { ...playback, ...change } })
}

/** Plays a playlist's or album's songs (or an artist's), from `index` (`null`: from the start, or anywhere on
 * shuffle). */
export function playFrom(context: SpotifyItem, tracks: SpotifyTrack[], index: number | null): void {
  if (tracks.length === 0) return
  const cover =
    index !== null && tracks[index] ? { ...context, image: tracks[index].image, color: tracks[index].color } : context
  if (!signedInOr({ reason: 'play', cover: context.kind === 'artist' || context.kind === 'liked' ? context : cover }))
    return
  const was = useSpotify.getState().playback
  const shuffle = was?.shuffle ?? false
  const start = index ?? (shuffle && !isInHost() ? Math.floor(Math.random() * tracks.length) : 0)
  if (isInHost()) {
    spotify({ action: 'play', context: context.uri, track: index === null ? undefined : tracks[index].uri })
    expectPlaying()
  }
  useSpotify.setState({
    playback: {
      context: contextOf(context),
      tracks: isInHost() ? tracks.slice(start) : tracks,
      index: isInHost() ? 0 : start,
      playing: true,
      position: 0,
      at: performance.now(),
      shuffle,
      repeat: was?.repeat ?? 'off',
      volume: was?.volume ?? (isInHost() ? null : PREVIEW_VOLUME),
      here: true,
      direction: 1,
    },
  })
}

/** A song on its own (a Home card, a search's song): plays from its album, from it. */
export function playSong(song: SpotifyItem): void {
  if (!signedInOr({ reason: 'play', cover: song })) return
  const album = song.context ?? ''
  if (isInHost()) {
    spotify({ action: 'play', context: album || song.uri, track: song.uri })
    expectPlaying()
    return
  }
  ask({ ask: 'collection', uri: album }).then(
    (page) => {
      const index = page.tracks.findIndex((track) => track.uri === song.uri)
      playFrom(page.item, page.tracks, Math.max(0, index))
    },
    () => {},
  )
}

/** Plays or pauses what's playing; or, pressed on a page that isn't what plays, plays it. */
export function playOrPause(context?: SpotifyItem, tracks?: SpotifyTrack[]): void {
  const playback = useSpotify.getState().playback
  if (context && tracks && playback?.context?.uri !== context.uri) {
    playFrom(context, tracks, null)
    return
  }
  if (!playback) return
  const now = performance.now()
  if (isInHost()) {
    spotify({ action: playback.playing ? 'pause' : 'play' })
    if (!playback.playing) expectPlaying()
  }
  showPlayback({ playing: !playback.playing, position: positionOf(playback, now), at: now })
}

/** The next song, or the one before (within a song's first seconds; later, its start). In the preview, the
 * app's own choice (a shuffle, any other song; past the last, the first again only on repeat). */
export function skip(direction: 1 | -1): void {
  const playback = useSpotify.getState().playback
  if (!playback) return
  const now = performance.now()
  if (isInHost()) {
    lastSkip = { direction, at: now }
    spotify({ action: direction === 1 ? 'next' : 'previous' })
    return
  }
  if (direction === -1 && positionOf(playback, now) > 3) {
    showPlayback({ position: 0, at: now })
    return
  }
  const count = playback.tracks.length
  let index = playback.index + direction
  if (playback.shuffle && direction === 1 && count > 1) {
    index = (playback.index + 1 + Math.floor(Math.random() * (count - 1))) % count
  }
  if (index >= count) {
    if (playback.repeat === 'off') {
      showPlayback({ index: 0, playing: false, position: 0, at: now })
      return
    }
    index = 0
  }
  showPlayback({ index: Math.max(0, index), position: 0, at: now, direction })
}

/** A song of the queue pressed (`offset` songs on): it plays, from where it is. */
export function jumpTo(offset: number): void {
  const playback = useSpotify.getState().playback
  const song = playback?.tracks[playback.index + offset]
  if (!playback || !song) return
  if (isInHost()) {
    if (playback.context) spotify({ action: 'play', context: playback.context.uri, track: song.uri })
    else for (let i = 0; i < offset; i++) spotify({ action: 'next' })
    showPlayback({
      tracks: playback.tracks.slice(playback.index + offset),
      index: 0,
      position: 0,
      at: performance.now(),
      direction: 1,
    })
    return
  }
  showPlayback({ index: playback.index + offset, position: 0, at: performance.now(), direction: 1 })
}

/** How long Spotify has to say it plays what was asked for, before the app says it couldn't. */
const PLAY_WAIT = 8000
let playWait: ReturnType<typeof setTimeout> | undefined

/** Playing asked for: unless Spotify says it plays by `PLAY_WAIT`, the app stops showing it playing (and says
 * it couldn't, as Spotify's own app does). */
function expectPlaying(): void {
  clearTimeout(playWait)
  playWait = setTimeout(() => {
    const playback = useSpotify.getState().playback
    const now = performance.now()
    useSpotify.setState({
      playback: playback ? { ...playback, playing: false, position: positionOf(playback, now), at: now } : null,
      unplayed: useSpotify.getState().unplayed + 1,
    })
  }, PLAY_WAIT)
}

/** The preview's volume to start with. */
const PREVIEW_VOLUME = 0.8
let volumeSent = 0

/** The playing device's volume (0 to 1), as it's dragged: shown at once, sent to Spotify a few times a second
 * (and the last one always). */
export function setVolume(share: number, done = false): void {
  const volume = Math.min(1, Math.max(0, share))
  showPlayback({ volume })
  if (!isInHost()) return
  const now = performance.now()
  if (!done && now - volumeSent < 150) return
  volumeSent = now
  spotify({ action: 'volume', percent: Math.round(volume * 100) })
}

/** The last skip pressed (for the way the cover slides when Spotify says the song changed). */
let lastSkip: { direction: 1 | -1; at: number } | null = null

export function seek(seconds: number): void {
  if (isInHost()) spotify({ action: 'seek', seconds: Math.max(0, seconds) })
  showPlayback({ position: Math.max(0, seconds), at: performance.now() })
}

export function toggleShuffle(): void {
  const playback = useSpotify.getState().playback
  if (!playback) return
  if (isInHost()) spotify({ action: 'shuffle', on: !playback.shuffle })
  showPlayback({ shuffle: !playback.shuffle })
}

/** Off, the whole list, the one song: Spotify's order. */
export function cycleRepeat(): void {
  const playback = useSpotify.getState().playback
  if (!playback) return
  const repeat: Repeat = playback.repeat === 'off' ? 'all' : playback.repeat === 'all' ? 'one' : 'off'
  if (isInHost()) spotify({ action: 'repeat', mode: repeat })
  showPlayback({ repeat })
}

/** The end of a preview song: the next, or the same again on repeat one. */
function songEnded(): void {
  const playback = useSpotify.getState().playback
  if (!playback) return
  if (playback.repeat === 'one') showPlayback({ position: 0, at: performance.now() })
  else skip(1)
}

/** What Spotify plays (`spotifyPlayback`): the app follows it. */
/** How long after a source starts playing Home is asked again (Spotify's Recently Played takes a moment). */
const PLAYED_AFTER = 6000
let playedSource: string | null = null
let playedLater: ReturnType<typeof setTimeout> | undefined

export function receivePlayback(playback: SpotifyPlayback | null): void {
  if (!playback?.track) {
    useSpotify.setState({ playback: null, nowPlaying: false })
    return
  }
  const was = useSpotify.getState().playback
  const before = currentTrack(was)
  const now = performance.now()
  // The cover slides the way the song changed: back if Back was pressed a moment ago, forward otherwise.
  const backwards = lastSkip?.direction === -1 && now - lastSkip.at < 4000
  const direction: 1 | -1 = before?.uri === playback.track.uri ? (was?.direction ?? 1) : backwards ? -1 : 1
  if (before?.uri !== playback.track.uri) lastSkip = null
  if (playback.playing) clearTimeout(playWait)
  const context = playback.context
  // Another source started playing: Home's Recently Played asked again once Spotify has counted it.
  const source = context?.uri ?? playback.track.uri
  if (playback.playing && source !== playedSource) {
    playedSource = source
    clearTimeout(playedLater)
    playedLater = setTimeout(() => useSpotify.setState({ played: useSpotify.getState().played + 1 }), PLAYED_AFTER)
  }
  useSpotify.setState({
    playback: {
      context: context ? { ...context, kind: context.kind === 'track' ? 'album' : context.kind } : null,
      tracks: [playback.track, ...playback.next],
      index: 0,
      playing: playback.playing,
      position: playback.position,
      at: now,
      shuffle: playback.shuffle,
      repeat: playback.repeat,
      volume: playback.volume,
      here: playback.here ?? true,
      direction,
    },
  })
}

let toastId = 0
let toastTimer: ReturnType<typeof setTimeout> | undefined

export function showToast(text: string): void {
  clearTimeout(toastTimer)
  useSpotify.setState({ toast: { text, id: ++toastId } })
  toastTimer = setTimeout(() => useSpotify.setState({ toast: null }), 2600)
}

/** Whether a song is in Liked Songs, or a page in Your Library: as changed here, else as Spotify said. */
export function useSaved(uri: string, said: boolean | undefined): boolean {
  return useSpotify((state) => state.saved[uri] ?? said ?? false)
}

/** Adds to (or takes out of) Liked Songs or Your Library, saying so. */
export function toggleSaved(uri: string, saved: boolean, words: { added: string; removed: string }): void {
  if (!signedInOr({ reason: 'save', cover: null })) return
  if (isInHost()) spotify({ action: 'save', uri, on: !saved })
  useSpotify.setState({ saved: { ...useSpotify.getState().saved, [uri]: !saved } })
  showToast(saved ? words.removed : words.added)
}

/** Making a playlist (Your Library's first card): signed out, the prompt says to log in first. */
export function createPlaylist(): void {
  signedInOr({ reason: 'playlist', cover: null })
}

/** What Spotify plays, as the Dashboard's Now Playing takes music (a media source, as Apple Music's and YouTube
 * Music's), and its cover; `null` while nothing does. */
export function useSpotifySource(): { source: MediaSource; artwork: string | null } | null {
  const playback = useSpotify((state) => state.playback)
  return useMemo(() => {
    const track = currentTrack(playback)
    if (!playback || !track) return null
    return {
      source: {
        id: 'westplay-spotify',
        service: 'spotify',
        title: track.name,
        artist: track.artists.map((artist) => artist.name).join(', '),
        album: track.album.name,
        artwork: null,
        status: playback.playing ? 'playing' : 'paused',
        // As Spotify said it, in Unix time: the Dashboard runs it on from there.
        positionMs: playback.position * 1000,
        positionAt: Math.round(performance.timeOrigin + playback.at),
        durationMs: track.duration * 1000,
        controls: { playPause: true, next: true, previous: true, shuffle: true, repeat: true },
        shuffle: playback.shuffle,
        repeat: playback.repeat === 'one' ? 'track' : playback.repeat === 'all' ? 'list' : 'off',
      },
      artwork: track.image,
    }
  }, [playback])
}

/** Whether music plays in WestPlay itself now (not on another of the account's devices). */
export function playingHere(): boolean {
  const playback = useSpotify.getState().playback
  return Boolean(playback?.playing && playback.here)
}

/** Pauses what plays here (the game paused, tabbed out of, its electrics off). */
export function pauseHere(): void {
  const playback = useSpotify.getState().playback
  if (!playback?.here) return
  const now = performance.now()
  if (isInHost()) spotify({ action: 'pause' })
  showPlayback({ playing: false, position: positionOf(playback, now), at: now })
}

/** Plays on what was paused here (back in the game). */
export function resumeHere(): void {
  const playback = useSpotify.getState().playback
  if (!playback?.here || playback.playing) return
  if (isInHost()) {
    spotify({ action: 'play' })
    expectPlaying()
  }
  showPlayback({ playing: true, at: performance.now() })
}

/** The time (`performance.now()`), again every quarter second while `running`: for a song's time. */
export function useNow(running: boolean): number {
  const [now, setNow] = useState(() => performance.now())
  useEffect(() => {
    if (!running) return
    const timer = setInterval(() => setNow(performance.now()), 250)
    return () => clearInterval(timer)
  }, [running])
  return now
}

/** While the app is open: Spotify's web player runs (hidden, under the page) and says whether it's signed
 * in. */
export function useWebPlayer(): void {
  useEffect(() => {
    if (isInHost()) spotify({ action: 'start' })
  }, [])
}

/** While the app is open in a plain browser: the preview's songs end by themselves, the next one starting
 * (in WestPlay, Spotify plays on by itself). */
export function useSongsEnd(): void {
  const playing = useSpotify((state) => state.playback?.playing ?? false)
  useEffect(() => {
    if (!playing || isInHost()) return
    const timer = setInterval(() => {
      const playback = useSpotify.getState().playback
      const track = currentTrack(playback)
      if (playback && track && positionOf(playback) >= track.duration) songEnded()
    }, 400)
    return () => clearInterval(timer)
  }, [playing])
}
