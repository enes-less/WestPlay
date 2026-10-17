import { useEffect, useMemo, useState } from 'react'
import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'
import {
  type YouTubeMusicAnswers,
  type YouTubeMusicBrowse,
  type YouTubeMusicCommand,
  type YouTubeMusicItem,
  type YouTubeMusicPlayback,
  type YouTubeMusicQuestion,
  type YouTubeMusicRating,
  type YouTubeMusicRepeat,
  type YouTubeMusicShelf,
  type YouTubeMusicTrack,
  type YouTubeMusicWatch,
  isInHost,
  postToHost,
} from '../../../shared/host'
import type { MediaSource } from '../../../shared/media'
import { createAsking } from '../asking'
import { positionOf, type Timed } from '../music/time'
import { keptAccount } from '../youtube/account'
import type { Hole } from '../youtube/youtube'
import { previewAnswer, previewPlay } from './preview'

export { positionOf } from '../music/time'
export type {
  YouTubeMusicArtist,
  YouTubeMusicChip,
  YouTubeMusicCollection,
  YouTubeMusicItem,
  YouTubeMusicPage,
  YouTubeMusicResults,
  YouTubeMusicSection,
  YouTubeMusicShelf,
  YouTubeMusicTrack,
} from '../../../shared/host'

/**
 * The YouTube Music app's state: WestPlay's own app in the look of YouTube Music's own tablet app. YouTube
 * Music's web player, run by the host in a hidden WebView (crates/host youtube_music.rs), plays and answers the
 * app's questions (Home, Explore, a search, an album's songs) from its own page. The app shows what it asked for
 * at once (a song pressed shows here before YouTube Music says it plays), then follows what YouTube Music says
 * plays. In a plain browser an invented preview answers and plays (preview.ts).
 *
 * Signed out, YouTube Music is what its web player is then: Home, Explore and Search, every song playing. The
 * sign-in is YouTube's (they share it): Google's own window on the PC's desktop. The host says whether it's
 * signed in and the account's picture, nothing more; those two are kept (shown at once as WestPlay
 * starts, replaced as YouTube Music's page says them). Answers are only shown, held for this session.
 */

export type Question = YouTubeMusicQuestion
export type Answers = YouTubeMusicAnswers
export type Repeat = YouTubeMusicRepeat

export type Tab = 'home' | 'explore' | 'library' | 'search'
/** The tabs of the bottom bar, in its order (Search is the top bar's). */
export const TABS: readonly Tab[] = ['home', 'explore', 'library']

/** A page over a tab's own (the card pressed, to show at once while its page comes). */
export type View =
  | { id: number; kind: 'collection'; item: YouTubeMusicItem }
  | { id: number; kind: 'artist'; item: YouTubeMusicItem }
  | { id: number; kind: 'browse'; title: string; page: YouTubeMusicBrowse; color?: string }
  | { id: number; kind: 'results'; query: string }
  | { id: number; kind: 'library'; shelf: YouTubeMusicShelf }

/** What plays: the song, playing or not (or waiting for its sound), how far and how long, the options, the
 * Song/Video switch, the queue around it, what it plays from, an ad playing, and the way the last change of song
 * went (Now Playing's cover slides that way). */
export interface Playback extends Timed {
  track: YouTubeMusicTrack
  shuffle: boolean
  repeat: Repeat
  volume: number | null
  video: boolean
  queue: YouTubeMusicItem[]
  start: number
  index: number
  from: string
  ad: boolean
  direction: 1 | -1
}

/** Why the sign-in prompt shows: to see the Library, to like a song, to save to the Library. */
export type Prompt = 'library' | 'like' | 'save'

/** Now Playing's sheet: the queue, the song's words, what goes with it. */
export type Panel = 'upNext' | 'lyrics' | 'related'

interface YouTubeMusicState {
  /** Whether YouTube Music's web player is signed in (the host says; kept as last said): `null` until known. */
  signedIn: boolean | null
  playing: boolean | null
  signingIn: boolean
  /** The account's picture (Google's), only shown (and kept with the sign-in). */
  avatar: string | null

  tab: Tab
  /** Each tab's pages over its own, the one showing last (each tab is kept where it was left). */
  views: Record<Tab, View[]>
  /** Home's mood chosen (its params), or none (all of Home). */
  chip: string | null
  /** Now Playing is open, over the app; its sheet (Up next, Lyrics, Related) risen over it, or not. */
  nowPlaying: boolean
  panel: Panel | null
  /** A song's ⋮ open: its choices, in a sheet over the app. */
  menu: YouTubeMusicItem | null
  /** The picture behind Home's top (as YouTube Music's Home says it, for its mood). */
  backdrop: string | null
  playback: Playback | null
  /** Likes as changed in this session, by song (YouTube Music's own say the rest). */
  likes: Record<string, YouTubeMusicRating>
  /** Saved to the Library (or taken out) in this session, by playlist. */
  saved: Record<string, boolean>
  toast: { text: string; id: number } | null
  /** Searches made, newest first (kept, as YouTube's app keeps its own). */
  searches: string[]
  prompt: Prompt | null
  /** Plays YouTube Music didn't confirm, counted (the app says it couldn't play, each time). */
  unplayed: number
  /** Where the music video's picture shows through the page (Now Playing's, its Video side), in the CarPlay
   * area's pixels; the shell cuts the area open there (Shell.tsx). */
  hole: Hole | null
}

const NO_VIEWS: Record<Tab, View[]> = { home: [], explore: [], library: [], search: [] }
const SEARCHES_KEPT = 12

export const useYouTubeMusic = create<YouTubeMusicState>()(
  persist(
    (): YouTubeMusicState => ({
      // Outside WestPlay (a plain browser) there's no host to say: the preview starts signed out.
      signedIn: isInHost() ? null : false,
      playing: null,
      signingIn: false,
      avatar: null,
      tab: 'home',
      views: NO_VIEWS,
      chip: null,
      nowPlaying: false,
      panel: null,
      menu: null,
      backdrop: null,
      playback: null,
      likes: {},
      saved: {},
      toast: null,
      searches: [],
      prompt: null,
      unplayed: 0,
      hole: null,
    }),
    {
      name: 'westplay.youtubeMusic',
      version: 1,
      storage: createJSONStorage(() => localStorage),
      // Signed in or not and the account's picture (shown at once as WestPlay starts), and
      // the searches made. Nothing else.
      partialize: (state) => ({ signedIn: state.signedIn, avatar: state.avatar, searches: state.searches }),
      // Kept by an earlier WestPlay (or edited): only what's sound.
      merge: (stored, current) => {
        const { signedIn, avatar, searches } = (stored ?? {}) as Partial<YouTubeMusicState>
        const account = isInHost() ? keptAccount({ signedIn, avatar }) : null
        return {
          ...current,
          ...(account ? { signedIn: account.signedIn, avatar: account.avatar } : {}),
          searches: Array.isArray(searches)
            ? searches.filter((s) => typeof s === 'string' && s.length <= 200).slice(0, SEARCHES_KEPT)
            : [],
        }
      },
    },
  ),
)

export function youtubeMusic(command: YouTubeMusicCommand): void {
  postToHost({ type: 'youtubeMusic', command })
}

// --- The account -------------------------------------------------------------------------------------------

/** How long the app waits for the web player to say before showing YouTube Music signed out. */
const ACCOUNT_WAIT = 8000

/** Signed in, signed out, or not known yet (the web player starting, nothing kept): known by `ACCOUNT_WAIT` at
 * the latest, signed out then. */
export function useAccount(): 'in' | 'out' | 'unknown' {
  const signedIn = useYouTubeMusic((state) => state.signedIn)
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

/** Google's sign-in (YouTube's), in a window on the PC's desktop. */
export function signIn(): void {
  useYouTubeMusic.setState({ prompt: null })
  if (isInHost()) {
    youtubeMusic({ action: 'signIn' })
    return
  }
  useYouTubeMusic.setState({ signingIn: true })
  setTimeout(() => useYouTubeMusic.setState({ signingIn: false, signedIn: true }), PREVIEW_SIGN_IN)
}

/** Signs out (YouTube too: they share the sign-in). */
export function signOut(): void {
  if (isInHost()) youtubeMusic({ action: 'signOut' })
  useYouTubeMusic.setState({ signedIn: false, avatar: null })
}

/** Signed in, or the sign-in prompt shows (why): whether to go on. */
function signedInOr(prompt: Prompt): boolean {
  if (useYouTubeMusic.getState().signedIn) return true
  useYouTubeMusic.setState({ prompt })
  return false
}

/** What the host says of YouTube Music's web player (`youtubeMusic`). */
export function receiveState(message: {
  signedIn: boolean | null
  playing: boolean | null
  signingIn: boolean
  avatar: string | null
}): void {
  const { signedIn, avatar } = useYouTubeMusic.getState()
  useYouTubeMusic.setState({
    playing: message.playing,
    signingIn: message.signingIn,
    // Not running (`null`): the kept sign-in stays as it was.
    ...(message.signedIn === null
      ? {}
      : {
          signedIn: message.signedIn,
          // The picture comes a moment after the sign-in: the kept one stays till then.
          avatar: message.signedIn ? (message.avatar ?? (signedIn ? avatar : null)) : null,
        }),
  })
}

// Signed in or out: nothing's kept of what the account had (its answers asked again, for the other).
useYouTubeMusic.subscribe((state, before) => {
  if (state.signedIn !== before.signedIn && before.signedIn !== null && state.signedIn !== null) {
    asking.clear()
    useYouTubeMusic.setState({
      playback: null,
      nowPlaying: false,
      likes: {},
      saved: {},
      views: NO_VIEWS,
      chip: null,
      tab: state.tab === 'library' && !state.signedIn ? 'home' : state.tab,
    })
  }
})

// --- Questions ---------------------------------------------------------------------------------------------

const asking = createAsking<Question, Answers>(
  (id, question) => youtubeMusic({ action: 'ask', id, question }),
  previewAnswer,
  isInHost,
)
export const ask = asking.ask
export const useAsked = asking.useAsked
/** YouTube Music's answer (`youtubeMusicAnswer`), for the question asked with its id. */
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

/** A tab: pressed again where it already is, back to its own page. */
export function openTab(tab: Tab): void {
  const { tab: was, views } = useYouTubeMusic.getState()
  if (was === tab) useYouTubeMusic.setState({ views: { ...views, [tab]: [] } })
  else useYouTubeMusic.setState({ tab })
}

type Unidentified<T> = T extends unknown ? Omit<T, 'id'> : never

export function openView(view: Unidentified<View>): void {
  const { tab, views } = useYouTubeMusic.getState()
  useYouTubeMusic.setState({
    views: { ...views, [tab]: [...views[tab], { ...view, id: nextId++ } as View] },
    nowPlaying: false,
  })
}

export function goBack(): void {
  const { tab, views } = useYouTubeMusic.getState()
  useYouTubeMusic.setState({ views: { ...views, [tab]: views[tab].slice(0, -1) } })
}

/** A card or row pressed: its page; a song, a video or an episode plays. */
export function openItem(item: YouTubeMusicItem): void {
  if (item.kind === 'song' || item.kind === 'video' || item.kind === 'episode') {
    play(item)
    return
  }
  if (!item.browse) {
    if (item.play) play(item)
    return
  }
  if (item.kind === 'album' || item.kind === 'playlist') openView({ kind: 'collection', item })
  else if (item.kind === 'artist' || item.kind === 'channel') openView({ kind: 'artist', item })
  else openView({ kind: 'browse', title: item.title, page: item.browse, color: item.color })
}

/** A page of YouTube Music's by its id (a song's album, its artist). */
export function openPage(kind: 'album' | 'artist', page: YouTubeMusicBrowse, title: string): void {
  const item: YouTubeMusicItem = { kind, title, subtitle: '', browse: page }
  openView(kind === 'album' ? { kind: 'collection', item } : { kind: 'artist', item })
}

export function remember(query: string): void {
  const trimmed = query.trim()
  if (!trimmed) return
  const searches = useYouTubeMusic.getState().searches.filter((s) => s !== trimmed)
  useYouTubeMusic.setState({ searches: [trimmed, ...searches].slice(0, SEARCHES_KEPT) })
}

export function forget(query: string): void {
  useYouTubeMusic.setState({ searches: useYouTubeMusic.getState().searches.filter((s) => s !== query) })
}

// --- Playing -----------------------------------------------------------------------------------------------

/** A change of what plays, shown at once (YouTube Music then says how it really is). */
function showPlayback(change: Partial<Playback>): void {
  const playback = useYouTubeMusic.getState().playback
  if (playback) useYouTubeMusic.setState({ playback: { ...playback, ...change } })
}

/** "3:08" as seconds. */
export function secondsOf(length: string | undefined): number {
  const parts = (length ?? '').split(':').map(Number)
  return parts.every((part) => Number.isFinite(part)) ? parts.reduce((total, part) => total * 60 + part, 0) : 0
}

/** A song as Now Playing shows it, from its card or row (until YouTube Music says it). */
export function trackOf(item: YouTubeMusicItem): YouTubeMusicTrack {
  return {
    video: item.video ?? item.play?.video ?? '',
    title: item.title,
    artists: item.subtitle.split(' • ')[0] ?? '',
    ...(item.artist ? { artist: item.artist } : {}),
    album: '',
    ...(item.album ? { albumPage: item.album } : {}),
    ...(item.image ? { image: item.image } : {}),
    duration: secondsOf(item.duration),
    kind: item.kind,
    like: 'none',
    hasVideo: false,
  }
}

/** Plays what `watch` says, showing `shown` at once (a song pressed, or a list's first: its card). */
export function playWatch(watch: YouTubeMusicWatch, shown: YouTubeMusicTrack | null): void {
  const was = useYouTubeMusic.getState().playback
  if (!isInHost()) {
    const queue = previewPlay(watch)
    if (queue) startPreview(queue.items, queue.index, queue.from)
    return
  }
  youtubeMusic({ action: 'play', watch })
  expectPlaying()
  const track = shown ?? was?.track
  if (!track) return
  useYouTubeMusic.setState({
    playback: {
      track,
      playing: true,
      loading: true,
      position: 0,
      at: performance.now(),
      duration: track.duration,
      shuffle: was?.shuffle ?? false,
      repeat: was?.repeat ?? 'off',
      volume: was?.volume ?? null,
      video: was?.video ?? false,
      queue: [],
      start: 0,
      index: 0,
      from: '',
      ad: false,
      direction: 1,
    },
  })
}

/** A song, video or episode pressed: it plays (from its list, if it was in one). A list's card: it plays. */
export function play(item: YouTubeMusicItem): void {
  if (!item.play) return
  const song = item.kind === 'song' || item.kind === 'video' || item.kind === 'episode'
  playWatch(item.play, song ? trackOf(item) : null)
}

/** Plays or pauses what's playing. */
export function playOrPause(): void {
  const playback = useYouTubeMusic.getState().playback
  if (!playback) return
  const now = performance.now()
  if (isInHost()) {
    youtubeMusic({ action: playback.playing ? 'pause' : 'play' })
    if (playback.playing) stopExpecting()
  }
  showPlayback({ playing: !playback.playing, position: positionOf(playback, now), at: now })
}

/** The last skip pressed (for the way the cover slides when YouTube Music says the song changed). */
let lastSkip: { direction: 1 | -1; at: number } | null = null

/** The next song, or the one before (within a song's first seconds; later, its start). */
export function skip(direction: 1 | -1): void {
  const playback = useYouTubeMusic.getState().playback
  if (!playback) return
  const now = performance.now()
  if (!isInHost()) {
    previewSkip(direction)
    return
  }
  lastSkip = { direction, at: now }
  youtubeMusic({ action: direction === 1 ? 'next' : 'previous' })
  // Back late in a song is its start: no waiting. Otherwise the next song's: its time held, its bar breathing.
  if (direction === -1 && positionOf(playback, now) > 3) showPlayback({ position: 0, at: now })
  else if (playback.playing) showPlayback({ loading: true, position: positionOf(playback, now), at: now })
}

/** A song of the queue pressed (its place in the whole queue): it plays. */
export function jumpTo(index: number): void {
  const playback = useYouTubeMusic.getState().playback
  const song = playback?.queue[index - playback.start]
  if (!playback || !song) return
  if (!isInHost()) {
    startPreview(playback.queue, index - playback.start, playback.from)
    return
  }
  youtubeMusic({ action: 'jump', index })
  showPlayback({
    track: trackOf(song),
    index,
    playing: true,
    loading: true,
    position: 0,
    at: performance.now(),
    duration: secondsOf(song.duration),
    direction: index < playback.index ? -1 : 1,
  })
}

export function seek(seconds: number): void {
  if (isInHost()) youtubeMusic({ action: 'seek', seconds: Math.max(0, seconds) })
  showPlayback({ position: Math.max(0, seconds), at: performance.now() })
}

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
  youtubeMusic({ action: 'volume', percent: Math.round(volume * 100) })
}

/** YouTube Music's shuffle: its whole queue shuffled, played from its first song (as its web player does). */
export function toggleShuffle(): void {
  const playback = useYouTubeMusic.getState().playback
  if (!playback) return
  if (isInHost()) youtubeMusic({ action: 'shuffle', on: !playback.shuffle })
  showPlayback({ shuffle: !playback.shuffle })
}

/** Off, all, the one song: YouTube Music's order. */
export function cycleRepeat(): void {
  const playback = useYouTubeMusic.getState().playback
  if (!playback) return
  const repeat: Repeat = playback.repeat === 'off' ? 'all' : playback.repeat === 'all' ? 'one' : 'off'
  if (isInHost()) youtubeMusic({ action: 'repeat', mode: repeat })
  showPlayback({ repeat })
}

/** A song's radio: YouTube Music's own, made from the song. */
export function startRadio(item: YouTubeMusicItem): void {
  const video = item.video ?? item.play?.video
  if (!video) return
  playWatch({ video, playlist: `RDAMVM${video}`, params: 'wAEB' }, trackOf(item))
}

/** A song into the queue (right after the one playing, or at its end), saying so. */
export function queueSong(item: YouTubeMusicItem, next: boolean, words: { next: string; end: string }): void {
  const video = item.video ?? item.play?.video
  if (!video) return
  // Nothing plays: it plays (as YouTube Music's own menu does, with an empty queue).
  if (!useYouTubeMusic.getState().playback) {
    play(item)
    return
  }
  if (isInHost()) youtubeMusic({ action: 'queue', video, next })
  else {
    const playback = useYouTubeMusic.getState().playback
    if (playback) {
      const queue = [...playback.queue]
      queue.splice(next ? playback.index - playback.start + 1 : queue.length, 0, item)
      showPlayback({ queue })
    }
  }
  showToast(next ? words.next : words.end)
}

/** The Song/Video switch. */
export function setVideo(on: boolean): void {
  if (isInHost()) youtubeMusic({ action: 'video', on })
  showPlayback({ video: on })
}

/** A song's like: as changed here, else as YouTube Music said. */
export function useRating(track: YouTubeMusicTrack): YouTubeMusicRating {
  return useYouTubeMusic((state) => state.likes[track.video] ?? track.like)
}

/** Likes (or dislikes) a song, or takes its like back (pressed again). */
export function rate(track: YouTubeMusicTrack, rating: 'like' | 'dislike', was: YouTubeMusicRating): void {
  if (!signedInOr('like') || !track.video) return
  const like: YouTubeMusicRating = was === rating ? 'none' : rating
  if (isInHost()) youtubeMusic({ action: 'like', video: track.video, like })
  useYouTubeMusic.setState({ likes: { ...useYouTubeMusic.getState().likes, [track.video]: like } })
}

/** Whether an album or playlist is in the Library: as changed here, else as YouTube Music said. */
export function useSaved(playlist: string | undefined, said: boolean | undefined): boolean {
  return useYouTubeMusic((state) => (playlist ? (state.saved[playlist] ?? said ?? false) : false))
}

/** Saves an album or playlist to the Library (or takes it out), saying so. */
export function save(playlist: string, on: boolean, words: { saved: string; removed: string }): void {
  if (!signedInOr('save')) return
  if (isInHost()) youtubeMusic({ action: 'save', playlist, on })
  useYouTubeMusic.setState({ saved: { ...useYouTubeMusic.getState().saved, [playlist]: on } })
  showToast(on ? words.saved : words.removed)
}

let toastId = 0
let toastTimer: ReturnType<typeof setTimeout> | undefined

export function showToast(text: string): void {
  clearTimeout(toastTimer)
  useYouTubeMusic.setState({ toast: { text, id: ++toastId } })
  toastTimer = setTimeout(() => useYouTubeMusic.setState({ toast: null }), 2600)
}

/** How long YouTube Music has to say it plays what was asked for, before the app says it couldn't. */
const PLAY_WAIT = 15000
let playWait: ReturnType<typeof setTimeout> | undefined
/** Playing was asked for, and YouTube Music hasn't said it plays yet (what it says meanwhile, that nothing or
 * nothing new plays, is on its way there: not shown). */
let awaitingPlay = false

function expectPlaying(): void {
  clearTimeout(playWait)
  awaitingPlay = true
  playWait = setTimeout(() => {
    awaitingPlay = false
    const playback = useYouTubeMusic.getState().playback
    const now = performance.now()
    useYouTubeMusic.setState({
      playback: playback
        ? { ...playback, playing: false, loading: false, position: positionOf(playback, now), at: now }
        : null,
      unplayed: useYouTubeMusic.getState().unplayed + 1,
    })
  }, PLAY_WAIT)
}

function stopExpecting(): void {
  clearTimeout(playWait)
  awaitingPlay = false
}

/** What YouTube Music plays (`youtubeMusicPlayback`): the app follows it. */
export function receivePlayback(playback: YouTubeMusicPlayback | null): void {
  // On the way to playing what was asked for: what's said meanwhile isn't where it's going.
  if (awaitingPlay && !playback?.playing) return
  if (!playback) {
    stopExpecting()
    useYouTubeMusic.setState({ playback: null, nowPlaying: false })
    return
  }
  const was = useYouTubeMusic.getState().playback
  const now = performance.now()
  const changed = was?.track.video !== playback.track.video
  // The cover slides the way the song changed: back if Back was pressed a moment ago, forward otherwise.
  const backwards = lastSkip?.direction === -1 && now - lastSkip.at < 4000
  const direction: 1 | -1 = !changed ? (was?.direction ?? 1) : backwards || playback.index < (was?.index ?? 0) ? -1 : 1
  if (changed) lastSkip = null
  if (playback.playing) stopExpecting()
  useYouTubeMusic.setState({
    playback: {
      track: playback.track,
      playing: playback.playing,
      loading: playback.loading,
      position: playback.position,
      at: now,
      duration: playback.duration || playback.track.duration,
      shuffle: playback.shuffle,
      repeat: playback.repeat,
      volume: playback.volume,
      video: playback.video,
      queue: playback.queue,
      start: playback.start,
      index: playback.index,
      from: playback.from,
      ad: playback.ad,
      direction,
    },
  })
}

// --- The preview's own playing (a plain browser: no host) ----------------------------------------------------

function startPreview(items: YouTubeMusicItem[], index: number, from: string): void {
  const song = items[index]
  if (!song) return
  const was = useYouTubeMusic.getState().playback
  useYouTubeMusic.setState({
    playback: {
      // The preview's music videos have a picture (their cover stands in for it).
      track: { ...trackOf(song), hasVideo: song.kind === 'video' },
      playing: true,
      loading: false,
      position: 0,
      at: performance.now(),
      duration: secondsOf(song.duration) || 200,
      shuffle: was?.shuffle ?? false,
      repeat: was?.repeat ?? 'off',
      volume: was?.volume ?? 0.8,
      video: was?.video ?? false,
      queue: items,
      start: 0,
      index,
      from,
      ad: false,
      direction: index < (was?.index ?? 0) ? -1 : 1,
    },
  })
}

function previewSkip(direction: 1 | -1): void {
  const playback = useYouTubeMusic.getState().playback
  if (!playback) return
  const now = performance.now()
  if (direction === -1 && positionOf(playback, now) > 3) {
    showPlayback({ position: 0, at: now })
    return
  }
  const next = playback.index + direction
  if (next < 0) return
  if (next >= playback.queue.length) {
    if (playback.repeat === 'all') startPreview(playback.queue, 0, playback.from)
    else showPlayback({ playing: false, position: 0, at: now })
    return
  }
  startPreview(playback.queue, next, playback.from)
}

/** While the app is open in a plain browser: the preview's songs end by themselves, the next one starting. */
export function useSongsEnd(): void {
  const playing = useYouTubeMusic((state) => state.playback?.playing ?? false)
  useEffect(() => {
    if (!playing || isInHost()) return
    const timer = setInterval(() => {
      const playback = useYouTubeMusic.getState().playback
      if (!playback || positionOf(playback) < playback.duration) return
      if (playback.repeat === 'one') showPlayback({ position: 0, at: performance.now() })
      else previewSkip(1)
    }, 400)
    return () => clearInterval(timer)
  }, [playing])
}

// --- With the game, the Dashboard -----------------------------------------------------------------------------

/** What YouTube Music plays, as the Dashboard's Now Playing takes music, and its cover; `null` while nothing
 * does. */
export function useYouTubeMusicSource(): { source: MediaSource; artwork: string | null } | null {
  const playback = useYouTubeMusic((state) => state.playback)
  return useMemo(() => {
    if (!playback) return null
    return {
      source: {
        id: 'westplay-youtube-music',
        service: 'youTubeMusic',
        title: playback.track.title,
        artist: playback.track.artists,
        album: playback.track.album,
        artwork: null,
        status: playback.playing ? 'playing' : 'paused',
        positionMs: playback.position * 1000,
        positionAt: Math.round(performance.timeOrigin + playback.at),
        durationMs: playback.duration * 1000,
        controls: { playPause: true, next: true, previous: true, shuffle: true, repeat: true },
        shuffle: playback.shuffle,
        repeat: playback.repeat === 'one' ? 'track' : playback.repeat === 'all' ? 'list' : 'off',
      },
      artwork: playback.track.image ?? null,
    }
  }, [playback])
}

export function playingNow(): boolean {
  return Boolean(useYouTubeMusic.getState().playback?.playing)
}

/** Pauses what plays (the game paused, tabbed out of, its electrics off). */
export function pauseNow(): void {
  const playback = useYouTubeMusic.getState().playback
  if (!playback?.playing) return
  const now = performance.now()
  if (isInHost()) {
    youtubeMusic({ action: 'pause' })
    stopExpecting()
  }
  showPlayback({ playing: false, position: positionOf(playback, now), at: now })
}

/** Plays on what was paused (back in the game). */
export function resumeNow(): void {
  const playback = useYouTubeMusic.getState().playback
  if (!playback || playback.playing) return
  if (isInHost()) youtubeMusic({ action: 'play' })
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

/** While the app is open: YouTube Music's web player runs (hidden, under the page), answering in the cab's
 * language, and says whether it's signed in. */
export function useWebPlayer(language: 'en' | 'tr'): void {
  useEffect(() => {
    if (!isInHost()) return
    youtubeMusic({ action: 'start' })
  }, [])
  useEffect(() => {
    if (!isInHost()) return
    youtubeMusic({ action: 'language', code: language })
    // Answers in the other language are another's.
    asking.clear()
  }, [language])
}
