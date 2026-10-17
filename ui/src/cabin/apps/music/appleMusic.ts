import { useEffect, useMemo, useState } from 'react'
import { create } from 'zustand'
import { createAsking } from '../asking'
import {
  type AppleMusicAnswers,
  type AppleMusicCommand,
  type AppleMusicItem,
  type AppleMusicPlayback,
  type AppleMusicQuestion,
  type AppleMusicRepeat,
  type AppleMusicSection,
  type AppleMusicShelf,
  type AppleMusicTrack,
  isInHost,
  postToHost,
} from '../../../shared/host'
import type { MediaSource } from '../../../shared/media'
import { previewAnswer, previewQueue } from './preview'
import { positionOf } from './time'

export { positionOf } from './time'

export type {
  AppleMusicArtist,
  AppleMusicCollection,
  AppleMusicItem,
  AppleMusicLibrary,
  AppleMusicPage,
  AppleMusicResults,
  AppleMusicSection,
  AppleMusicShelf,
  AppleMusicTrack,
} from '../../../shared/host'

/**
 * The Music app's state: Apple Music, WestPlay's own app in the look of Apple's own (iPadOS 26's Music, its
 * Now Playing CarPlay's). Apple Music's web player, run by the host in a hidden WebView (crates/host
 * apple_music.rs), plays and answers the app's questions (Home, New, Radio, the Library, a search, an album's
 * songs) from its own page, through MusicKit. The app shows what it asked for at once (a song pressed plays
 * here before Apple Music says so), then follows what Apple Music says plays. In a plain browser an invented
 * preview answers and plays (preview.ts).
 *
 * Signed out, Apple Music is what its web player is then: New, Radio and Search, every song playing as its
 * 90-second preview. Signing in is MusicKit's own window on the PC's desktop. The host says whether it's
 * signed in, nothing more of the account. Nothing of the account is kept: answers are only shown (and held for
 * this session, to go back to).
 */

export type Question = AppleMusicQuestion
export type Answers = AppleMusicAnswers
export type Repeat = AppleMusicRepeat

export type Tab = 'home' | 'new' | 'radio' | 'library' | 'search'
export const TABS: readonly Tab[] = ['home', 'new', 'radio', 'library', 'search']

/** A page over a tab's own (the card pressed, to show at once while its page comes). */
export type View =
  | { id: number; kind: 'collection'; item: AppleMusicItem }
  | { id: number; kind: 'artist'; item: AppleMusicItem }
  | { id: number; kind: 'curator'; item: AppleMusicItem }
  | { id: number; kind: 'results'; query: string }
  | { id: number; kind: 'library'; shelf: AppleMusicShelf; folder?: AppleMusicItem }
  /** A section's See All: all of it. */
  | { id: number; kind: 'section'; section: AppleMusicSection }

/** Where what plays plays from. */
export interface PlayContext {
  id: string
  name: string
  kind: AppleMusicItem['kind']
}

/** What plays: the song (`index` of `tracks`), those after it, where it plays from (`null`: a song on its
 * own, or not known), and how far. */
export interface Playback {
  context: PlayContext | null
  tracks: AppleMusicTrack[]
  index: number
  playing: boolean
  /** Playing as asked, its sound not come yet: its time held still (its bar breathes). */
  loading: boolean
  /** Seconds into the song at `at` (`performance.now()`), running on from there while it plays. */
  position: number
  at: number
  /** How long it plays: the song's length, or its preview's (90 seconds). */
  duration: number
  preview: boolean
  shuffle: boolean
  repeat: Repeat
  /** The player's volume, 0 to 1 (`null`: not known yet). */
  volume: number | null
  /** The way the last change of song went (Now Playing's cover slides that way). */
  direction: 1 | -1
}

/** How a list of the Library shows: as a grid or a list, sorted (`null`: as Apple Music sorts it), all of it or
 * only the favourites. */
export interface LibraryLook {
  view: 'grid' | 'list'
  sort: string | null
  filter: 'all' | 'favorites'
}

/** Each list as it starts (Apple's): playlists and songs as lists, albums and artists as grids. */
const LIBRARY_LOOKS: Record<AppleMusicShelf, LibraryLook> = {
  recent: { view: 'grid', sort: null, filter: 'all' },
  playlists: { view: 'list', sort: null, filter: 'all' },
  albums: { view: 'grid', sort: null, filter: 'all' },
  artists: { view: 'list', sort: null, filter: 'all' },
  songs: { view: 'list', sort: null, filter: 'all' },
}

/** Why the sign-in prompt shows: to play in full, to keep something in the Library, to favourite. */
export type Prompt = 'library' | 'favorite'

interface AppleMusicState {
  /** Whether Apple Music's web player is signed in (the host says): `null` until it knows. */
  signedIn: boolean | null
  playing: boolean | null
  signingIn: boolean
  /** The account's picture (its Apple Music profile's), only shown. */
  avatar: string | null

  tab: Tab
  /** Each tab's pages over its own, the one showing last (each tab is kept where it was left). */
  views: Record<Tab, View[]>
  /** Now Playing is open, over the app. */
  nowPlaying: boolean
  /** Now Playing shows Up Next in the cover's place. */
  queue: boolean
  /** Now Playing shows the song's words in the cover's place. */
  lyrics: boolean
  playback: Playback | null
  /** Favourites as changed in this session, by song (Apple Music's own answers say the rest). */
  favorites: Record<string, boolean>
  /** Added to the Library in this session, by id. */
  added: Record<string, boolean>
  /** "Added to Library": the toast showing. */
  toast: { text: string; id: number } | null
  /** Searches' picks, newest first: Recently Searched. */
  recent: AppleMusicItem[]
  /** The sign-in prompt, over the app. */
  prompt: Prompt | null
  /** Plays Apple Music didn't confirm, counted (the app says it couldn't play, each time). */
  unplayed: number
  /** How each list of the Library shows, as chosen in this session. */
  libraryLooks: Record<AppleMusicShelf, LibraryLook>
}

const NO_VIEWS: Record<Tab, View[]> = { home: [], new: [], radio: [], library: [], search: [] }

export const useAppleMusic = create<AppleMusicState>()(() => ({
  // Outside WestPlay (a plain browser) there's no host to say: the preview starts signed out.
  signedIn: isInHost() ? null : false,
  playing: null,
  signingIn: false,
  avatar: null,
  tab: 'home',
  views: NO_VIEWS,
  nowPlaying: false,
  queue: false,
  lyrics: false,
  playback: null,
  favorites: {},
  added: {},
  toast: null,
  recent: [],
  prompt: null,
  unplayed: 0,
  libraryLooks: LIBRARY_LOOKS,
}))

/** A change of how a list of the Library shows. */
export function setLibraryLook(shelf: AppleMusicShelf, change: Partial<LibraryLook>): void {
  const looks = useAppleMusic.getState().libraryLooks
  useAppleMusic.setState({ libraryLooks: { ...looks, [shelf]: { ...looks[shelf], ...change } } })
}

export function appleMusic(command: AppleMusicCommand): void {
  postToHost({ type: 'appleMusic', command })
}

// --- The account -------------------------------------------------------------------------------------------

/** How long the app waits for the web player to say before showing Apple Music signed out. */
const ACCOUNT_WAIT = 8000

/** Signed in, signed out, or not known yet (the web player starting): known by `ACCOUNT_WAIT` at the latest,
 * signed out then. */
export function useAccount(): 'in' | 'out' | 'unknown' {
  const signedIn = useAppleMusic((state) => state.signedIn)
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

/** MusicKit's sign-in, in a window on the PC's desktop. */
export function signIn(): void {
  useAppleMusic.setState({ prompt: null })
  if (isInHost()) {
    appleMusic({ action: 'signIn' })
    return
  }
  useAppleMusic.setState({ signingIn: true })
  setTimeout(() => useAppleMusic.setState({ signingIn: false, signedIn: true }), PREVIEW_SIGN_IN)
}

export function signOut(): void {
  if (isInHost()) appleMusic({ action: 'signOut' })
  else useAppleMusic.setState({ signedIn: false })
}

/** Signed in, or the sign-in prompt shows (why): whether to go on. */
function signedInOr(prompt: Prompt): boolean {
  if (useAppleMusic.getState().signedIn) return true
  useAppleMusic.setState({ prompt })
  return false
}

// Signed in or out: nothing's kept of what the account had (its answers asked again, for the other).
useAppleMusic.subscribe((state, before) => {
  if (state.signedIn !== before.signedIn && before.signedIn !== null) {
    asking.clear()
    useAppleMusic.setState({
      playback: null,
      nowPlaying: false,
      favorites: {},
      added: {},
      recent: [],
      views: NO_VIEWS,
      tab: state.signedIn ? state.tab : state.tab === 'library' || state.tab === 'home' ? 'new' : state.tab,
    })
  }
})

// --- Questions ---------------------------------------------------------------------------------------------

const asking = createAsking<Question, Answers>(
  (id, question) => appleMusic({ action: 'ask', id, question }),
  previewAnswer,
  isInHost,
)
/** A question's answer: as known this session, or (`fresh`, or not known) asked of Apple Music. */
export const ask = asking.ask
/** A question's answer once it comes (`null` until then), whether Apple Music failed to answer, and asking
 * again; asked again when the question changes. A page whose answer changes as Apple Music is used (Home's
 * Recently Played, the Library) gives a `refresh`: it's asked afresh when that changes (the page shown again)
 * and whenever the answer is known already (the page opened anew), the known answer shown meanwhile. */
export const useAsked = asking.useAsked
/** Apple Music's answer (`appleMusicAnswer`), for the question asked with its id. */
export const receiveAnswer = asking.receive

/** How many times a page has been shown again (a page kept under others, or a tab left, is shown again). */
export function useShownAgain(shown: boolean): number {
  const [times, setTimes] = useState(0)
  const [was, setWas] = useState(shown)
  if (shown !== was) {
    setWas(shown)
    if (shown) setTimes(times + 1)
  }
  return times
}

// --- Going around ------------------------------------------------------------------------------------------

let nextId = 1

/** A tab: pressed again where it already is, back to its own page (Apple's way). */
export function openTab(tab: Tab): void {
  const { tab: was, views } = useAppleMusic.getState()
  if (was === tab) useAppleMusic.setState({ views: { ...views, [tab]: [] } })
  else useAppleMusic.setState({ tab })
}

type Unidentified<T> = T extends unknown ? Omit<T, 'id'> : never

export function openView(view: Unidentified<View>): void {
  const { tab, views } = useAppleMusic.getState()
  useAppleMusic.setState({ views: { ...views, [tab]: [...views[tab], { ...view, id: nextId++ } as View] } })
}

/** A card pressed: its page; a station or a song plays. */
export function openItem(item: AppleMusicItem): void {
  if (item.kind === 'station' || item.kind === 'song') play(item)
  else if (item.kind === 'folder') openView({ kind: 'library', shelf: 'playlists', folder: item })
  else if (item.kind === 'artist') openView({ kind: 'artist', item })
  else if (item.kind === 'curator') openView({ kind: 'curator', item })
  else openView({ kind: 'collection', item })
}

/** A song's album, its page (Now Playing's album name, a song's own row). */
export function openAlbum(track: AppleMusicTrack): void {
  if (!track.albumId) return
  openView({
    kind: 'collection',
    item: {
      id: track.albumId,
      kind: 'album',
      name: track.album,
      by: track.artist,
      note: '',
      image: track.image,
      wide: null,
      color: track.color,
      explicit: false,
      live: false,
    },
  })
}

export function openArtist(id: string, name: string): void {
  openView({
    kind: 'artist',
    item: {
      id,
      kind: 'artist',
      name,
      by: '',
      note: '',
      image: null,
      wide: null,
      color: '#2c2c2e',
      explicit: false,
      live: false,
    },
  })
}

export function goBack(): void {
  const { tab, views } = useAppleMusic.getState()
  useAppleMusic.setState({ views: { ...views, [tab]: views[tab].slice(0, -1) } })
}

export function remember(item: AppleMusicItem): void {
  const recent = useAppleMusic.getState().recent.filter((r) => r.id !== item.id)
  useAppleMusic.setState({ recent: [item, ...recent].slice(0, 10) })
}

export function clearRecent(): void {
  useAppleMusic.setState({ recent: [] })
}

// --- Playing -----------------------------------------------------------------------------------------------

export function currentTrack(playback: Playback | null): AppleMusicTrack | null {
  return playback ? (playback.tracks[playback.index] ?? null) : null
}

/** A change of what plays, shown at once (Apple Music then says how it really is). */
function showPlayback(change: Partial<Playback>): void {
  const playback = useAppleMusic.getState().playback
  if (playback) useAppleMusic.setState({ playback: { ...playback, ...change } })
}

/** The preview's songs play their preview only, signed out (as Apple Music's do). */
const PREVIEW_LENGTH = 90

function lengthOf(track: AppleMusicTrack): number {
  return useAppleMusic.getState().signedIn ? track.duration : Math.min(PREVIEW_LENGTH, track.duration)
}

/** Plays an album's or playlist's songs (or a list of songs: `context` null), from `index` (`null`: from the
 * start, or anywhere on shuffle). */
export function playFrom(
  context: AppleMusicItem | null,
  tracks: AppleMusicTrack[],
  index: number | null,
  shuffle?: boolean,
): void {
  if (tracks.length === 0) return
  const was = useAppleMusic.getState().playback
  const shuffled = shuffle ?? was?.shuffle ?? false
  const start = index ?? (shuffled ? Math.floor(Math.random() * tracks.length) : 0)
  if (isInHost()) {
    // A list of the Library's songs, or a search's, plays as that list; an album or playlist as itself.
    const list = !context || context.kind === 'artist'
    appleMusic({
      action: 'play',
      ...(list ? { songs: tracks.map((track) => track.id) } : { id: context.id }),
      // The song to start with, chosen here (a random one, shuffling): Apple Music starts with it.
      index: start,
      shuffle: shuffled,
    })
    expectPlaying()
  }
  useAppleMusic.setState({
    playback: {
      context: context ? { id: context.id, name: context.name, kind: context.kind } : null,
      tracks,
      index: start,
      playing: true,
      loading: isInHost(),
      position: 0,
      at: performance.now(),
      duration: lengthOf(tracks[start]),
      preview: !useAppleMusic.getState().signedIn,
      shuffle: shuffled,
      repeat: was?.repeat ?? 'off',
      volume: was?.volume ?? (isInHost() ? null : PREVIEW_VOLUME),
      direction: 1,
    },
  })
}

/** A station or a song on its own (a card): it plays. A song plays from its album, from it. */
export function play(item: AppleMusicItem): void {
  if (isInHost()) {
    appleMusic({ action: 'play', id: item.id })
    expectPlaying()
    const was = useAppleMusic.getState().playback
    const track: AppleMusicTrack = {
      id: item.id,
      name: item.name,
      artist: item.by,
      album: '',
      albumId: null,
      artistId: null,
      image: item.image,
      color: item.color,
      duration: 0,
      explicit: item.explicit,
      number: null,
    }
    useAppleMusic.setState({
      playback: {
        context: item.kind === 'station' ? { id: item.id, name: item.name, kind: 'station' } : null,
        tracks: [track],
        index: 0,
        playing: true,
        loading: true,
        position: 0,
        at: performance.now(),
        duration: 0,
        preview: false,
        shuffle: was?.shuffle ?? false,
        repeat: was?.repeat ?? 'off',
        volume: was?.volume ?? null,
        direction: 1,
      },
    })
    return
  }
  const queue = previewQueue(item)
  if (queue) playFrom(queue.context, queue.tracks, Math.max(0, queue.index))
}

/** Plays or pauses what's playing. */
export function playOrPause(): void {
  const playback = useAppleMusic.getState().playback
  if (!playback) return
  const now = performance.now()
  if (isInHost()) {
    appleMusic({ action: playback.playing ? 'pause' : 'play' })
    if (playback.playing) stopExpecting()
    else expectPlaying()
  }
  showPlayback({
    playing: !playback.playing,
    loading: isInHost() && !playback.playing,
    position: positionOf(playback, now),
    at: now,
  })
}

/** The next song, or the one before (within a song's first seconds; later, its start). In the preview, the
 * app's own choice (a shuffle, any other song; past the last, the first again only on repeat). */
export function skip(direction: 1 | -1): void {
  const playback = useAppleMusic.getState().playback
  if (!playback) return
  const now = performance.now()
  if (isInHost()) {
    lastSkip = { direction, at: now }
    appleMusic({ action: direction === 1 ? 'next' : 'previous' })
    // Waiting for the next song: its time held, its bar breathing.
    if (playback.playing) showPlayback({ loading: true, position: positionOf(playback, now), at: now })
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
  const next = Math.max(0, index)
  showPlayback({ index: next, position: 0, at: now, direction, duration: lengthOf(playback.tracks[next]) })
}

/** A song of Up Next pressed (`offset` songs on): it plays, from where it is. */
export function jumpTo(offset: number): void {
  const playback = useAppleMusic.getState().playback
  const song = playback?.tracks[playback.index + offset]
  if (!playback || !song) return
  if (isInHost()) appleMusic({ action: 'jump', offset })
  showPlayback({
    loading: isInHost(),
    index: playback.index + offset,
    position: 0,
    at: performance.now(),
    direction: 1,
    duration: lengthOf(song),
  })
}

/** How long Apple Music has to say it plays what was asked for, before the app says it couldn't. */
const PLAY_WAIT = 10000
let playWait: ReturnType<typeof setTimeout> | undefined
/** Playing was asked for, and Apple Music hasn't said it plays yet (what it says meanwhile, that nothing or
 * nothing new plays, is on its way there: not shown). */
let awaitingPlay = false

/** Playing asked for: unless Apple Music says it plays by `PLAY_WAIT`, the app stops showing it playing (and
 * says it couldn't). */
function expectPlaying(): void {
  clearTimeout(playWait)
  awaitingPlay = true
  playWait = setTimeout(() => {
    awaitingPlay = false
    const playback = useAppleMusic.getState().playback
    const now = performance.now()
    useAppleMusic.setState({
      playback: playback ? { ...playback, playing: false, position: positionOf(playback, now), at: now } : null,
      unplayed: useAppleMusic.getState().unplayed + 1,
    })
  }, PLAY_WAIT)
}

/** Paused: no play is awaited any more. */
function stopExpecting(): void {
  clearTimeout(playWait)
  awaitingPlay = false
}

/** The preview's volume to start with. */
const PREVIEW_VOLUME = 0.8
let volumeSent = 0

/** The player's volume (0 to 1), as it's dragged: shown at once, sent a few times a second (and the last one
 * always). */
export function setVolume(share: number, done = false): void {
  const volume = Math.min(1, Math.max(0, share))
  showPlayback({ volume })
  if (!isInHost()) return
  const now = performance.now()
  if (!done && now - volumeSent < 120) return
  volumeSent = now
  appleMusic({ action: 'volume', percent: Math.round(volume * 100) })
}

/** The last skip pressed (for the way the cover slides when Apple Music says the song changed). */
let lastSkip: { direction: 1 | -1; at: number } | null = null

export function seek(seconds: number): void {
  if (isInHost()) appleMusic({ action: 'seek', seconds: Math.max(0, seconds) })
  showPlayback({ position: Math.max(0, seconds), at: performance.now() })
}

export function toggleShuffle(): void {
  const playback = useAppleMusic.getState().playback
  if (!playback) return
  if (isInHost()) appleMusic({ action: 'shuffle', on: !playback.shuffle })
  showPlayback({ shuffle: !playback.shuffle })
}

/** Off, all, the one song: Apple's order. */
export function cycleRepeat(): void {
  const playback = useAppleMusic.getState().playback
  if (!playback) return
  const repeat: Repeat = playback.repeat === 'off' ? 'all' : playback.repeat === 'all' ? 'one' : 'off'
  if (isInHost()) appleMusic({ action: 'repeat', mode: repeat })
  showPlayback({ repeat })
}

/** The end of a preview song: the next, or the same again on repeat one. */
function songEnded(): void {
  const playback = useAppleMusic.getState().playback
  if (!playback) return
  if (playback.repeat === 'one') showPlayback({ position: 0, at: performance.now() })
  else skip(1)
}

/** What Apple Music plays (`appleMusicPlayback`): the app follows it. */
export function receivePlayback(playback: AppleMusicPlayback | null): void {
  // On the way to playing what was asked for: what's said meanwhile isn't where it's going.
  if (awaitingPlay && !playback?.playing) return
  if (!playback?.track) {
    stopExpecting()
    useAppleMusic.setState({ playback: null, nowPlaying: false })
    return
  }
  const was = useAppleMusic.getState().playback
  const before = currentTrack(was)
  const now = performance.now()
  // The cover slides the way the song changed: back if Back was pressed a moment ago, forward otherwise.
  const backwards = lastSkip?.direction === -1 && now - lastSkip.at < 4000
  const direction: 1 | -1 = before?.id === playback.track.id ? (was?.direction ?? 1) : backwards ? -1 : 1
  if (before?.id !== playback.track.id) lastSkip = null
  if (playback.playing) stopExpecting()
  const { favorites } = useAppleMusic.getState()
  const favorite = playback.track.favorite
  useAppleMusic.setState({
    playback: {
      context: playback.context,
      tracks: [playback.track, ...playback.next],
      index: 0,
      playing: playback.playing,
      loading: playback.loading ?? false,
      position: playback.position,
      at: now,
      duration: playback.duration || playback.track.duration,
      preview: playback.preview,
      shuffle: playback.shuffle,
      repeat: playback.repeat,
      volume: playback.volume,
      direction,
    },
    // What Apple Music says of the song's favourite is the latest (not said: what was known stays).
    favorites: favorite == null ? favorites : { ...favorites, [playback.track.id]: favorite },
  })
}

let toastId = 0
let toastTimer: ReturnType<typeof setTimeout> | undefined

export function showToast(text: string): void {
  clearTimeout(toastTimer)
  useAppleMusic.setState({ toast: { text, id: ++toastId } })
  toastTimer = setTimeout(() => useAppleMusic.setState({ toast: null }), 2600)
}

/** Whether a song is a favourite: as changed here, else as Apple Music said. */
export function useFavorite(track: AppleMusicTrack): boolean {
  return useAppleMusic((state) => state.favorites[track.id] ?? track.favorite ?? false)
}

/** Favourites a song (or not), saying so. */
export function toggleFavorite(track: AppleMusicTrack, favorite: boolean, words: { on: string; off: string }): void {
  if (!signedInOr('favorite')) return
  if (isInHost()) appleMusic({ action: 'favorite', id: track.id, on: !favorite })
  useAppleMusic.setState({ favorites: { ...useAppleMusic.getState().favorites, [track.id]: !favorite } })
  showToast(favorite ? words.off : words.on)
}

/** Whether an album, playlist or song is in the Library (added in this session; the Library's own are). */
export function useAdded(id: string): boolean {
  return useAppleMusic((state) => id.startsWith('library-') || (state.added[id] ?? false))
}

/** Adds an album, playlist or song to the Library, saying so. */
export function addToLibrary(id: string, words: { added: string }): void {
  if (!signedInOr('library')) return
  if (isInHost()) appleMusic({ action: 'add', id })
  useAppleMusic.setState({ added: { ...useAppleMusic.getState().added, [id]: true } })
  showToast(words.added)
}

/** What Apple Music plays, as the Dashboard's Now Playing takes music, and its cover; `null` while nothing
 * does. */
export function useAppleMusicSource(): { source: MediaSource; artwork: string | null } | null {
  const playback = useAppleMusic((state) => state.playback)
  return useMemo(() => {
    const track = currentTrack(playback)
    if (!playback || !track) return null
    return {
      source: {
        id: 'westplay-apple-music',
        service: 'appleMusic',
        title: track.name,
        artist: track.artist,
        album: track.album,
        artwork: null,
        status: playback.playing ? 'playing' : 'paused',
        positionMs: playback.position * 1000,
        positionAt: Math.round(performance.timeOrigin + playback.at),
        durationMs: playback.duration * 1000,
        controls: { playPause: true, next: true, previous: true, shuffle: true, repeat: true },
        shuffle: playback.shuffle,
        repeat: playback.repeat === 'one' ? 'track' : playback.repeat === 'all' ? 'list' : 'off',
      },
      artwork: track.image,
    }
  }, [playback])
}

/** Whether Apple Music plays now. */
export function playingNow(): boolean {
  return Boolean(useAppleMusic.getState().playback?.playing)
}

/** Pauses what plays (the game paused, tabbed out of, its electrics off). */
export function pauseNow(): void {
  const playback = useAppleMusic.getState().playback
  if (!playback?.playing) return
  const now = performance.now()
  if (isInHost()) {
    appleMusic({ action: 'pause' })
    stopExpecting()
  }
  showPlayback({ playing: false, position: positionOf(playback, now), at: now })
}

/** Plays on what was paused (back in the game). */
export function resumeNow(): void {
  const playback = useAppleMusic.getState().playback
  if (!playback || playback.playing) return
  if (isInHost()) {
    appleMusic({ action: 'play' })
    expectPlaying()
  }
  showPlayback({ playing: true, loading: isInHost(), at: performance.now() })
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

/** While the app is open: Apple Music's web player runs (hidden, under the page) and says whether it's signed
 * in. */
export function useWebPlayer(): void {
  useEffect(() => {
    if (isInHost()) appleMusic({ action: 'start' })
  }, [])
}

/** While the app is open in a plain browser: the preview's songs end by themselves, the next one starting. */
export function useSongsEnd(): void {
  const playing = useAppleMusic((state) => state.playback?.playing ?? false)
  useEffect(() => {
    if (!playing || isInHost()) return
    const timer = setInterval(() => {
      const playback = useAppleMusic.getState().playback
      if (playback && positionOf(playback) >= playback.duration) songEnded()
    }, 400)
    return () => clearInterval(timer)
  }, [playing])
}
