import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'
import {
  isInHost,
  postToHost,
  type YouTubeAnswer,
  type YouTubeChannel,
  type YouTubeCommand,
  type YouTubePlaylist,
  type YouTubeState,
  type YouTubeVideo,
} from '../../../shared/host'
import { whenTransitionEnds } from '../../state/launch'
import { type Account, keptAccount } from './account'
import { follow, type Loading } from './follow'
import { previewAnswer } from './preview'

/**
 * The YouTube app's state: the video playing (YouTube's own player plays it, hidden under the page, in the
 * host: westplay_host::youtube), whether its player is open full screen, its Up next, and what's kept on
 * this PC (the searches made and the videos watched, for Home: YouTube has no Home for a player not signed
 * in).
 */
interface YouTubeStore {
  /** How YouTube's player plays (the host says); `null`: YouTube isn't running. And when it said so
   * (`performance.now()`), for the time to run on from there. */
  state: YouTubeState | null
  stateAt: number
  /** The video opened last (shown before YouTube's player has it, and after). */
  current: YouTubeVideo | null
  /** The player is open, full screen. */
  open: boolean
  /** The player was left open (the player went elsewhere, it didn't close it): YouTube opens on it again. */
  returnToPlayer: boolean
  /** Opened that way (onto a video already playing): the sidebar stays a moment before stepping aside. */
  holdSidebar: boolean
  /** The pages open over Home, the one showing last. */
  views: View[]
  /** The current video's Up next (`null` until YouTube has answered). */
  upNext: YouTubeVideo[] | null
  /** The playlist playing, in its order (Next and autoplay go through it, in place of Up next). */
  queue: Queue | null
  /** The captions for every video: the track last picked (its language on the videos after), or none. */
  captions: string | null
  /** The picture's quality for every video: `auto` (YouTube's own choice), or the most it's to be. */
  quality: string
  /** The playback speed, on every video until changed (not kept past WestPlay: YouTube's app's way). */
  rate: number
  /** The newest first. */
  searches: string[]
  /** The videos watched, newest first, and how far: for Continue watching. */
  watched: Watched[]
  /** The player's volume, 0-100, for every video, and whether it's muted (the volume kept for after). */
  volume: number
  muted: boolean
  /** Whether YouTube is signed in, and the picture for the player (the host says; `null`: not known yet).
   * Only shown, never kept. */
  account: Account | null
  /** Google's sign-in window is open on the PC's desktop. */
  signingIn: boolean
  /** Where the video shows through the page outside the full-screen player (Now playing's picture): the
   * CarPlay area is cut open there (Shell.tsx), in its own pixels. */
  hole: Hole | null
}

/** A rounded rectangle of the CarPlay area, in its CSS pixels. */
export interface Hole {
  x: number
  y: number
  width: number
  height: number
  radius: number
}

/** A playlist being played: its videos so far, and YouTube's token for more of them. */
export interface Queue {
  playlist: string
  title: string
  videos: YouTubeVideo[]
  more: string | null
}

export interface Watched {
  video: YouTubeVideo
  /** Seconds watched, of `duration`. */
  time: number
  duration: number
}

/** YouTube's qualities, from the least (`highres`: above 4K). */
export const QUALITIES = [
  'tiny',
  'small',
  'medium',
  'large',
  'hd720',
  'hd1080',
  'hd1440',
  'hd2160',
  'hd2880',
  'highres',
]

/** How many searches and videos are kept. */
const SEARCHES_KEPT = 8
const WATCHED_KEPT = 24

export const useYouTube = create<YouTubeStore>()(
  persist(
    (): YouTubeStore => ({
      state: null,
      stateAt: 0,
      current: null,
      open: false,
      returnToPlayer: false,
      holdSidebar: false,
      views: [],
      upNext: null,
      queue: null,
      captions: null,
      quality: 'auto',
      rate: 1,
      searches: [],
      watched: [],
      volume: 100,
      muted: false,
      // Outside WestPlay (a plain browser) there's no host to say: the preview starts signed out.
      account: isInHost() ? null : { signedIn: false, avatar: null },
      signingIn: false,
      hole: null,
    }),
    {
      name: 'westplay.youtube',
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: (store) => ({
        searches: store.searches,
        watched: store.watched,
        volume: store.volume,
        muted: store.muted,
        captions: store.captions,
        quality: store.quality,
        // Signed in or not, and the picture's address (shown at once as WestPlay starts,
        // not 5 to 7 seconds on as YouTube's page says it; replaced as it does, signed out on signing out).
        account: store.account,
      }),
      // Kept by an earlier WestPlay (or edited): only what's sound.
      merge: (stored, current) => {
        const { searches, watched, volume, muted, captions, quality, account } = (stored ?? {}) as Partial<YouTubeStore>
        return {
          ...current,
          account: keptAccount(account) ?? current.account,
          captions: typeof captions === 'string' && /^[\w.-]{1,64}$/.test(captions) ? captions : null,
          quality:
            typeof quality === 'string' && (quality === 'auto' || QUALITIES.includes(quality)) ? quality : 'auto',
          volume: typeof volume === 'number' && volume >= 0 && volume <= 100 ? Math.round(volume) : 100,
          muted: muted === true,
          searches: Array.isArray(searches)
            ? searches.filter((s) => typeof s === 'string').slice(0, SEARCHES_KEPT)
            : [],
          watched: Array.isArray(watched)
            ? watched.filter((w) => typeof w?.video?.video === 'string').slice(0, WATCHED_KEPT)
            : [],
        }
      },
    },
  ),
)

/** Tells YouTube's player, or YouTube, what to do. */
export function command(command: YouTubeCommand): void {
  if (command.action === 'show') shows++
  postToHost({ type: 'youtube', command })
}

/** How many times the picture has been placed (`show`), to tell whether it was placed again since. */
let shows = 0

// Questions for YouTube, by id, waiting for their answers.
let questions = 0
const waiting = new Map<number, (answer: YouTubeAnswer) => void>()
/** How long YouTube has to answer, ms. */
const ANSWER_MS = 20_000

export type Question =
  | { action: 'search'; query: string; more?: string; filter?: 'channels' | 'playlists' }
  | { action: 'suggest'; query: string }
  | { action: 'related'; video: string }
  | { action: 'feed'; which: 'home' | 'subscriptions'; more?: string }
  | { action: 'channel'; channel: string; more?: string }
  | { action: 'subscribe'; channel: string; on: boolean }
  | { action: 'playlist'; playlist: string; more?: string }

/** What a search shows: everything, or (YouTube's filters) channels or playlists only. */
export type SearchFilter = 'all' | 'channels' | 'playlists'

/** A page the app has open over Home: a search's results, a channel's or a playlist's (what's known of it so
 * far: at least its id and name). `id`: a new page each time. */
export type View =
  | { kind: 'results'; query: string; filter: SearchFilter; id: number }
  | { kind: 'channel'; channel: Pick<YouTubeChannel, 'channel' | 'name'> & Partial<YouTubeChannel>; id: number }
  | { kind: 'playlist'; playlist: KnownPlaylist; id: number }

/** What's known of a playlist before its page answers. */
export type KnownPlaylist = Pick<YouTubePlaylist, 'playlist' | 'title'> & Partial<YouTubePlaylist>

let views = 0

/** A search's results, over Home (over the results it replaces, if they're what's showing). */
export function openResults(query: string, filter: SearchFilter = 'all'): void {
  useYouTube.setState((store) => {
    const top = store.views.at(-1)
    const kept = top?.kind === 'results' ? store.views.slice(0, -1) : store.views
    return { views: [...kept, { kind: 'results', query, filter, id: ++views }] }
  })
}

/** A playlist's page, over what's open. */
export function openPlaylist(playlist: KnownPlaylist): void {
  useYouTube.setState((store) => ({ views: [...store.views, { kind: 'playlist', playlist, id: ++views }] }))
}

/** A channel's page, over what's open. */
export function openChannel(channel: View & { kind: 'channel' } extends { channel: infer C } ? C : never): void {
  useYouTube.setState((store) => ({ views: [...store.views, { kind: 'channel', channel, id: ++views }] }))
}

/** Back from the page on top. */
export function goBack(): void {
  useYouTube.setState((store) => ({ views: store.views.slice(0, -1) }))
}

/** Back to Home. */
export function goHome(): void {
  useYouTube.setState({ views: [] })
}

/** Asks YouTube (through its page in the host); `failed` when it doesn't answer in time. Outside the host
 * (a plain browser), invented videos answer. */
export function ask(question: Question): Promise<YouTubeAnswer> {
  if (!isInHost()) return previewAnswer(question)
  const id = ++questions
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      waiting.delete(id)
      resolve({ failed: true })
    }, ANSWER_MS)
    waiting.set(id, (answer) => {
      clearTimeout(timer)
      resolve(answer)
    })
    command({ ...question, id })
  })
}

export function receiveAnswer(id: number, answer: YouTubeAnswer): void {
  const answered = waiting.get(id)
  waiting.delete(id)
  answered?.(answer)
}

// The video asked for and not yet reported by YouTube's player (follow.ts).
let loading: Loading | null = null
// Whether a report has come since the page loaded (the first one puts YouTube's picture where it belongs).
let reported = false

/** What YouTube's player says. It's in the host, and outlives the page: what plays is what it says plays. */
export function receiveState(state: YouTubeState | null): void {
  const now = performance.now()
  useYouTube.setState({ state, stateAt: now })
  const { current: shown, open } = useYouTube.getState()
  const followed = follow(shown?.video ?? null, state?.video ?? null, loading, now)
  loading = followed.loading
  if (followed.shown === 'clear') useYouTube.setState({ current: null, open: false, upNext: null, queue: null })
  if (followed.shown === 'restore' && state?.video) restore(state)
  // The page (re)loaded with YouTube running: its picture is under the page until something shows it.
  if (state && !reported && !open) command({ action: 'hide' })
  reported = state !== null
  const { current } = useYouTube.getState()
  // How far the video playing is, for Continue watching (a live stream has no "how far").
  if (state && current && state.video === current.video && !state.live && state.duration > 0) {
    rememberProgress(current, state.time, state.duration)
  }
}

/** The video YouTube's player plays, shown as playing again (the page reloaded): as it was listed, if it was
 * watched here, otherwise from what the player says of it. */
function restore(state: YouTubeState): void {
  const video = state.video as string
  const watched = useYouTube.getState().watched.find((w) => w.video.video === video)?.video
  useYouTube.setState({
    current: watched ?? {
      video,
      title: state.title,
      channel: state.author,
      avatar: null,
      thumbnail: `https://i.ytimg.com/vi/${video}/hqdefault.jpg`,
      duration: '',
      views: '',
      age: '',
      live: state.live,
    },
    upNext: null,
  })
  fetchUpNext(video)
}

/** Asks for `video`'s Up next, kept if it's still the video playing when it comes. */
function fetchUpNext(video: string): void {
  ask({ action: 'related', video }).then((answer) => {
    if (useYouTube.getState().current?.video !== video) return
    useYouTube.setState({ upNext: (answer.videos ?? []).filter((v) => v.video !== video) })
  })
}

let rememberedAt = 0
/** How often the progress is written down, ms. */
const REMEMBER_MS = 5000

function rememberProgress(video: YouTubeVideo, time: number, duration: number): void {
  const now = Date.now()
  if (now - rememberedAt < REMEMBER_MS) return
  rememberedAt = now
  useYouTube.setState((store) => ({
    watched: store.watched.map((w) => (w.video.video === video.video ? { ...w, time, duration } : w)),
  }))
}

/** The language YouTube answers in: the cabin's. */
export function setLanguage(code: string): void {
  command({ action: 'language', code })
}

/** The volume as YouTube's player should have it: none while muted. */
function heard(): number {
  const { volume, muted } = useYouTube.getState()
  return muted ? 0 : volume
}

/** Sets the volume for every video (a volume above none unmutes). */
export function setVolume(percent: number): void {
  const volume = Math.round(Math.min(100, Math.max(0, percent)))
  useYouTube.setState({ volume, muted: volume === 0 })
  command({ action: 'volume', percent: volume })
}

/** Mutes, or brings the volume back. */
export function toggleMute(): void {
  const { muted, volume } = useYouTube.getState()
  // Unmuting a volume of none brings it back audible.
  useYouTube.setState({ muted: !muted, volume: muted && volume === 0 ? 50 : volume })
  command({ action: 'volume', percent: heard() })
}

/** Plays `video`, full screen unless `fullScreen` is false (from where it was left, if it was watched
 * before); in `queue` (a playlist), or in the one playing if it's one of its videos. */
export function play(video: YouTubeVideo, fullScreen = true, queue?: Queue): void {
  const before = useYouTube.getState().watched.find((w) => w.video.video === video.video)
  useYouTube.setState((store) => ({
    current: video,
    open: fullScreen || store.open,
    upNext: null,
    queue: queue ?? (store.queue?.videos.some((v) => v.video === video.video) ? store.queue : null),
    watched: [
      { video, time: before?.time ?? 0, duration: before?.duration ?? 0 },
      ...store.watched.filter((w) => w.video.video !== video.video),
    ].slice(0, WATCHED_KEPT),
  }))
  loading = { video: video.video, at: performance.now() }
  // The player's own volume, captions and quality first: no video starts at YouTube's.
  const { captions, quality, rate } = useYouTube.getState()
  command({ action: 'volume', percent: heard() })
  command({ action: 'captions', track: captions })
  command({ action: 'quality', quality })
  command({ action: 'rate', rate })
  // Picked up where it was left, unless it was (nearly) over (a playlist plays each from its start).
  const resume = !useYouTube.getState().queue && before && before.time > 10 && before.duration - before.time > 30
  command(resume ? { action: 'load', video: video.video, start: before.time } : { action: 'load', video: video.video })
  if (useYouTube.getState().queue) extendQueue()
  else fetchUpNext(video.video)
}

/** The playlist's next videos, asked for as it nears the end of those it has. */
function extendQueue(): void {
  const { queue, current } = useYouTube.getState()
  if (!queue?.more || !current) return
  const at = queue.videos.findIndex((v) => v.video === current.video)
  if (at < queue.videos.length - 3) return
  const asked = queue.more
  // Asked once: the token is the queue's until the answer replaces it.
  useYouTube.setState({ queue: { ...queue, more: null } })
  ask({ action: 'playlist', playlist: queue.playlist, more: asked }).then((answer) => {
    const now = useYouTube.getState().queue
    if (now?.playlist !== queue.playlist) return
    const seen = new Set(now.videos.map((v) => v.video))
    const added = (answer.videos ?? []).filter((v) => !seen.has(v.video))
    useYouTube.setState({
      queue: { ...now, videos: [...now.videos, ...added], more: answer.failed ? asked : (answer.more ?? null) },
    })
  })
}

/** The videos coming after the one playing: the playlist's next, or Up next. */
export function comingUp(store: Pick<YouTubeStore, 'queue' | 'upNext' | 'current'>): YouTubeVideo[] {
  if (!store.queue) return store.upNext ?? []
  const at = store.queue.videos.findIndex((v) => v.video === store.current?.video)
  return store.queue.videos.slice(at + 1)
}

/** The next video: the playlist's next (none past its end), or Up next's first not just watched. */
export function nextVideo(): YouTubeVideo | null {
  const store = useYouTube.getState()
  if (store.queue) return comingUp(store)[0] ?? null
  const recent = new Set(store.watched.slice(0, 5).map((w) => w.video.video))
  return store.upNext?.find((v) => !recent.has(v.video)) ?? store.upNext?.[0] ?? null
}

/** The video before this one: the playlist's, or the one watched before. */
export function previousVideo(): YouTubeVideo | null {
  const { queue, current, watched } = useYouTube.getState()
  const at = queue?.videos.findIndex((v) => v.video === current?.video) ?? -1
  if (queue && at > 0) return queue.videos[at - 1]
  return watched[1]?.video ?? null
}

/** Captions for every video: this track (on the video playing; its language after), or none. */
export function setCaptions(track: string | null): void {
  useYouTube.setState({ captions: track })
  command({ action: 'captions', track })
}

/** The playback speed, for the video playing and the videos after. */
export function setRate(rate: number): void {
  useYouTube.setState({ rate })
  command({ action: 'rate', rate })
}

/** The video's audio track (a dubbed one, or its original). */
export function setAudioTrack(id: string): void {
  command({ action: 'audioTrack', id })
}

/** YouTube's Stable volume, on or off (YouTube's player keeps it). */
export function setStableVolume(on: boolean): void {
  command({ action: 'stableVolume', on })
}

/** YouTube's Voice boost, on or off (YouTube's player keeps it). */
export function setVoiceBoost(on: boolean): void {
  command({ action: 'voiceBoost', on })
}

/** The picture's quality for every video (`auto`: YouTube's own choice). */
export function setQuality(quality: string): void {
  useYouTube.setState({ quality })
  command({ action: 'quality', quality })
}

/**
 * Calls `place` now, and again whenever where the video goes may have moved: the CarPlay area changing (the
 * OEM strip coming or going, the window resizing) or `element` (where the video shows) changing size (each
 * at most once a frame, as size changes are told). Returns the function that stops it.
 */
export function followArea(place: () => void, element?: Element): () => void {
  place()
  const resized = new ResizeObserver(() => place())
  const area = document.querySelector('.carplay')
  if (area) resized.observe(area)
  if (element) resized.observe(element)
  window.addEventListener('resize', place)
  return () => {
    resized.disconnect()
    window.removeEventListener('resize', place)
  }
}

/** Opens the full-screen player again on the video playing; `hold`: the sidebar stays a moment first (YouTube
 * opening onto it). */
export function reopenPlayer(hold = false): void {
  if (useYouTube.getState().current) useYouTube.setState({ open: true, returnToPlayer: false, holdSidebar: hold })
}

/** Leaves the full-screen player for elsewhere (another screen): the video plays on, and YouTube opens on it
 * again. */
export function leavePlayer(): void {
  useYouTube.setState({ open: false, returnToPlayer: useYouTube.getState().current !== null, holdSidebar: false })
  // The picture stays under the page while the screen changes (the Dashboard or Home fading in over it): the
  // player's screen lets it through all over, and without it the game's own screen would show there. Then it
  // goes, unless something has placed it again since (the Dashboard's Now Playing) or the player is back.
  const placed = shows
  whenTransitionEnds(() => {
    if (shows === placed && !useYouTube.getState().open) command({ action: 'hide' })
  })
}

/** Stops the video: paused, and no longer the video playing. */
export function stopVideo(): void {
  loading = null
  // YouTube closes (it's started again for the next video): nothing plays, after a reload too.
  command({ action: 'close' })
  useYouTube.setState({ current: null, open: false, returnToPlayer: false, upNext: null, queue: null })
}

/** Closes the full-screen player; the video plays on, heard. */
export function closePlayer(): void {
  useYouTube.setState({ open: false, returnToPlayer: false, holdSidebar: false })
  command({ action: 'hide' })
}

/** Opens Google's sign-in for YouTube in a window on the PC's desktop. */
export function signIn(): void {
  command({ action: 'signIn' })
  // Outside WestPlay, the preview's sign-in just succeeds.
  if (!isInHost()) useYouTube.setState({ account: { signedIn: true, avatar: null } })
}

/** Signs YouTube out (its cookies forgotten). */
export function signOut(): void {
  command({ action: 'signOut' })
  useYouTube.setState({ account: { signedIn: false, avatar: null } })
}

/** Remembers a search (the newest first, each once). */
export function rememberSearch(query: string): void {
  const trimmed = query.trim()
  if (!trimmed) return
  useYouTube.setState((store) => ({
    searches: [trimmed, ...store.searches.filter((s) => s.toLocaleLowerCase() !== trimmed.toLocaleLowerCase())].slice(
      0,
      SEARCHES_KEPT,
    ),
  }))
}

export function clearSearches(): void {
  useYouTube.setState({ searches: [] })
}

/** Forgets the videos watched (Continue watching). */
export function clearWatched(): void {
  useYouTube.setState({ watched: [] })
}

/** Where the video playing is now, in seconds: YouTube's last report, run on by the clock while it plays. */
export function positionNow(): number {
  const { state, stateAt } = useYouTube.getState()
  if (!state) return 0
  const run = state.state === 1 ? Math.max(0, (performance.now() - stateAt) / 1000) : 0
  return state.time + run
}

// The last seek: taps faster than YouTube reports each build on the one before.
let lastSeek = { to: 0, at: -Infinity }
/** How long a seek is where the video is, before YouTube has said so, ms. */
const SEEK_MEMORY_MS = 1500

/** Moves the video playing `seconds` on (or back), within it. */
export function seekBy(seconds: number): void {
  const { state } = useYouTube.getState()
  if (!state || state.live) return
  const now = performance.now()
  const from = now - lastSeek.at < SEEK_MEMORY_MS ? lastSeek.to : positionNow()
  const to = Math.max(0, from + seconds)
  const within = state.duration > 0 ? Math.min(to, state.duration - 1) : to
  lastSeek = { to: within, at: now }
  command({ action: 'seek', seconds: within })
}

export function forgetSearch(query: string): void {
  useYouTube.setState((store) => ({ searches: store.searches.filter((s) => s !== query) }))
}
