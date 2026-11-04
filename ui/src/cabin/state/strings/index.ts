import type { Sky } from '../../../shared/host'
import type { ReturnKind } from '../../keyboard/layouts'
import type { TurnKind } from '../../maps/guidance'
import type { DestinationKind } from '../../maps/places'
import type { AppId } from '../apps'

/**
 * The cabin's own words, in the game's languages (by the game's codes, `tr_tr`, as the companion's words):
 * one file each, in the game's language (or the one the player chose in the companion), English (`en_us`)
 * for a language without its file. Place names on the map come from the game's own text in its language
 * (crates/mapdata locale.rs), not from here.
 */

/** A game language code (`tr_tr`, `de_de`). */
export type Language = string

export interface Strings {
  /** The language's BCP 47 tag, for dates, numbers and capitals (`tr-TR`). */
  locale: string
  /** The decimal separator: "9.4 km", "9,4 km". */
  decimal: string
  /** The trip pill: "15:40 arrival · 12 min · 9.4 km". */
  arrival: string
  /** A 12-hour time with its time of day ("2:22 PM", "ÖS 2:22"): `afternoon` from noon. */
  timeOfDay: (time: string, afternoon: boolean) => string
  minutes: (minutes: number) => string
  hours: (hours: number, minutes: string) => string
  speedLimit: (limit: number) => string
  /** Turn-by-turn. */
  now: string
  then: string
  arrive: string
  turns: Record<Exclude<TurnKind, 'roundabout'>, string>
  roundabout: (exit: number) => string
  /** Maps. */
  recentre: string
  /** The compass: turns the map north up, seen from above. */
  northUp: string
  zoomIn: string
  zoomOut: string
  /** Maps' search: its button, and the field's hint. */
  search: string
  searchPlaces: string
  close: string
  /** Destinations (Maps' search): its sections, what places are (one, and the nearby lists'), the place
   * card's buttons, and how the game answered. */
  destinations: {
    favourites: string
    headquarters: string
    yourGarage: string
    findNearby: string
    /** "Find nearby" on a route: the places offered are along it. */
    alongRoute: string
    recents: string
    kinds: Record<DestinationKind, string>
    nearby: Partial<Record<DestinationKind, string>>
    straightLine: (distance: string) => string
    /** How far along the route a place is. */
    alongTheRoute: (distance: string) => string
    /** A stop added: how much longer the route is with it (the game's distance and time). */
    stopAdds: (distance: string, time: string) => string
    /** A stop added, but the game's new route doesn't pass it. */
    notOnRoute: string
    /** The routes offered before Go (Directions), by what each is planned for; while they're found. */
    plans: Record<'fastest' | 'shortest' | 'smallRoads' | 'other', string>
    finding: string
    /** Go: the route is set, but the game takes its own way there, not the one picked. */
    ownWay: string
    go: string
    addStop: string
    endRoute: string
    back: string
    noResults: string
    loading: string
    noPlaces: string
    done: Record<'go' | 'addStop' | 'end', string>
    alreadyThere: string
    full: string
    onJob: string
    noAnswer: string
    /** The game runs as administrator: WestPlay can't see its route. */
    elevated: string
    unsupported: string
    failed: string
  }
  /** The keyboard: its keys (named for screen readers where they show a symbol), and a field's clear button. */
  keyboard: {
    space: string
    returnKey: Record<ReturnKind, string>
    shift: string
    capsLock: string
    delete: string
    nextKeyboard: string
    hide: string
    clear: string
    /** Whether the player's own keyboard types into the field. */
    typingWithKeyboard: string
    pausedWhileDriving: string
    selectToType: string
  }
  /** Music. */
  nowPlaying: string
  sources: string
  previous: string
  play: string
  pause: string
  next: string
  shuffle: string
  repeat: string
  notPlaying: string
  openMusicApp: string
  nothingLoaded: string
  playing: string
  /** The Dashboard's destination widget: the job's pickup or delivery, or where the route ends. */
  delivery: string
  pickup: string
  destination: string
  mapPoint: string
  arriveBy: (time: string) => string
  late: string
  /** Days of the week, short, from Monday (the game's day 0). */
  days: readonly string[]
  /** The Dashboard's weather widget: the weather, and the weather it's turning to. */
  weather: Record<Sky, string>
  weatherTurning: Record<Sky, string>
  /** The YouTube app: the search field's hint, Home's sections and chips (each a search), what a list says
   * when it's empty or YouTube didn't answer, the live badge, and the player's words. */
  youtube: {
    search: string
    continueWatching: string
    recentSearches: string
    chips: readonly { label: string; query: string }[]
    start: string
    noResults: string
    failed: string
    tryAgain: string
    live: string
    upNext: string
    nowPlaying: string
    back: string
    volume: string
    forget: string
    fill: string
    clear: string
    fullScreen: string
    signIn: string
    signOut: string
    /** A search's results: all of them, or channels only. */
    all: string
    channels: string
    subscribe: string
    subscribed: string
    /** Playlists: a search's filter, the word under one found, and its page's button. */
    playlists: string
    playlist: string
    playAll: string
    /** The player's captions and quality, and their choices for none and YouTube's own. */
    captions: string
    off: string
    quality: string
    auto: string
    /** The player's gear: its settings, and in them the speed (and its 1×), the audio track (dubbed ones),
     * YouTube's Stable volume and Voice boost. */
    settings: string
    playbackSpeed: string
    normal: string
    audioTrack: string
    stableVolume: string
    voiceBoost: string
    home: string
    subscriptions: string
    /** While Google's sign-in window is open on the PC. */
    finishOnPc: string
    /** Double-tapping a side of the player: "10 seconds". */
    seconds: (seconds: number) => string
  }
  /** The Spotify app, in Spotify's own words: while its sign-in window is open on the PC; its tabs and the
   * search pill's hint; Home's filters; what things are ("Playlist • Spotify"); the pages' words; Now
   * Playing's; and the toasts of adding to Liked Songs or Your Library. */
  spotify: {
    finishOnPc: string
    whatToPlay: string
    home: string
    library: string
    browse: string
    back: string
    filters: { all: string; music: string; podcasts: string }
    showAll: string
    browseAll: string
    recentSearches: string
    clearRecent: string
    forget: string
    noResults: (query: string) => string
    noResultsHint: string
    topResult: string
    songs: string
    artists: string
    playlists: string
    albums: string
    podcasts: string
    kinds: { playlist: string; album: string; artist: string; show: string; liked: string; song: string }
    songCount: (count: number) => string
    /** A playlist's length: "2 hr 45 min", "38 min 12 sec". */
    length: (seconds: number) => string
    recents: string
    gridView: string
    listView: string
    verified: string
    follow: string
    following: string
    monthlyListeners: (count: string) => string
    popular: string
    discography: string
    playingFrom: { playlist: string; album: string; artist: string; show: string; liked: string }
    nowPlaying: string
    queue: string
    nextFrom: (name: string) => string
    nextUp: string
    /** Now Playing's words for the song (Spotify's Lyrics): none, not timed yet, and where they're from. */
    lyrics: string
    noLyrics: string
    notSynced: string
    lyricsFrom: (provider: string) => string
    /** Now Playing's volume. */
    volume: string
    mute: string
    unmute: string
    /** Spotify didn't play what was asked for. */
    cantPlay: string
    /** Spotify didn't answer. */
    failed: string
    tryAgain: string
    next: string
    previous: string
    shuffle: string
    repeat: string
    close: string
    more: string
    download: string
    addToLiked: string
    addToLibrary: string
    added: string
    removed: string
    addedToLibrary: string
    removedFromLibrary: string
    /** Signing in (Spotify's "Log in"), up and out; the account's button. */
    logIn: string
    signUp: string
    logOut: string
    account: string
    /** Signed out, at the foot: Spotify's banner for its preview. */
    previewTitle: string
    previewText: string
    signUpFree: string
    /** The log-in prompt, by what the player tried: to play, to save, to make a playlist. */
    prompts: {
      play: { title: string; text: string }
      save: { title: string; text: string }
      playlist: { title: string; text: string }
    }
    haveAccount: string
    notNow: string
    /** Your Library, signed out: Spotify's two first steps. */
    firstPlaylist: { title: string; text: string; button: string }
    firstPodcasts: { title: string; text: string; button: string }
    /** The "E" of an explicit song. */
    explicit: string
  }
  /** The Music app (Apple Music), in Apple's own words: its tabs and search field; its pages' and sections'
   * words; what things are ("Album · Tarkan"); its buttons, Now Playing's and Up Next's; its sign-in, and the
   * toasts of favouriting and adding to the Library. */
  appleMusic: {
    tabs: { home: string; new: string; radio: string; library: string; search: string }
    searchHint: string
    browseCategories: string
    recentlySearched: string
    clear: string
    topResults: string
    songs: string
    artists: string
    albums: string
    playlists: string
    stations: string
    noResults: string
    noResultsHint: string
    seeAll: string
    recentlyPlayed: string
    recentlyAdded: string
    kinds: {
      album: string
      playlist: string
      artist: string
      station: string
      song: string
      curator: string
      folder: string
    }
    /** "11 songs, 43 minutes" */
    length: (songs: number, seconds: number) => string
    topSongs: string
    play: string
    shuffle: string
    live: string
    explicit: string
    preview: string
    upNext: string
    /** Now Playing's words for the song (Apple Music's Lyrics), none, and who wrote them. */
    lyrics: string
    noLyrics: string
    writtenBy: (names: string) => string
    /** "From Blue Hour" (Up Next, the songs to come from where it plays). */
    from: (name: string) => string
    favorite: string
    favorited: string
    unfavorited: string
    addToLibrary: string
    added: string
    signIn: string
    signOut: string
    /** The account's button (its picture). */
    account: string
    /** A Library list's controls: the filter and More, their choices, and its search field's hint. */
    filter: string
    more: string
    all: { playlists: string; albums: string; songs: string }
    favorites: string
    grid: string
    list: string
    find: string
    /** The playlists' sorts (the other lists' are Apple Music's own words). */
    sorts: { name: string; '-dateAdded': string; '-lastPlayed': string; '-updated': string; type: string }
    /** While Apple Music's sign-in window is open on the PC. */
    finishOnPc: string
    /** Home, signed out. */
    homePitch: { title: string; text: string }
    /** The Library, signed out. */
    libraryPitch: { title: string; text: string }
    /** The sign-in prompt, and why. */
    promptTitle: string
    promptFor: { library: string; favorite: string }
    notNow: string
    /** The banner while songs play as their previews. */
    previewBanner: { title: string; text: string }
    empty: string
    cantPlay: string
    failed: string
    tryAgain: string
    back: string
    close: string
    nowPlaying: string
    next: string
    previous: string
    repeat: string
    volume: string
    mute: string
    unmute: string
  }
  /** YouTube Music: WestPlay's own app, in the look of YouTube Music's tablet app. */
  youtubeMusic: {
    tabs: { home: string; explore: string; library: string; search: string }
    searchHint: string
    remove: string
    more: string
    play: string
    shuffle: string
    radio: string
    save: string
    saved: string
    removed: string
    inLibrary: string
    like: string
    dislike: string
    /** The Song/Video switch. */
    song: string
    video: string
    upNext: string
    lyrics: string
    related: string
    playingFrom: string
    noLyrics: string
    shelves: {
      playlists: string
      songs: string
      albums: string
      artists: string
      subscriptions: string
      history: string
    }
    topResult: string
    /** "1.2M subscribers" */
    subscribers: (count: string) => string
    signIn: string
    signOut: string
    account: string
    /** While Google's sign-in window is open on the PC. */
    finishOnPc: string
    /** The Library, signed out. */
    libraryPitch: { title: string; text: string }
    /** The sign-in prompt, and why. */
    promptTitle: string
    promptFor: { library: string; like: string; save: string }
    notNow: string
    empty: string
    noResults: string
    cantPlay: string
    failed: string
    tryAgain: string
    back: string
    close: string
    nowPlaying: string
    pause: string
    next: string
    previous: string
    repeat: string
    /** An ad playing (YouTube Music's own, if one ever comes through). */
    ad: string
    /** A song's ⋮ and its choices, and what's said as they're made. */
    moreActions: string
    startRadio: string
    playNext: string
    addToQueue: string
    goToAlbum: string
    goToArtist: string
    willPlayNext: string
    addedToQueue: string
    volume: string
  }
  /** The Weather app: where the truck is, the game's next change of weather and the roads. */
  weatherApp: {
    myLocation: string
    nextChange: string
    /** "In about 1:42 hrs" */
    inAbout: (time: string) => string
    roads: string
    roadStates: { dry: string; damp: string; wet: string }
    roadTrends: { drying: string; gettingWet: string }
    /** No game, or one whose weather WestPlay can't read. */
    unavailable: string
  }
  /** The Settings app: its pages, rows and the words under them. */
  settings: {
    appearance: string
    light: string
    dark: string
    automatic: string
    automaticFooter: string
    /** The truck's information bar beside WestPlay (the DAF's, the Scania Smart Dash's), and what it is. */
    vehicleBar: string
    vehicleBarFooter: string
    textSize: string
    /** Text Size's own text, shown at the size chosen (iOS's). */
    textSizeAbout: string
    wallpaper: string
    dashboard: string
    widgets: string
    /** Settings › Dashboard: the map filling it while driving, and what that is. */
    mapWhileDriving: string
    mapWhileDrivingFooter: string
    showAlbumArt: string
    /** Date & Time, and in it 24-Hour Time (off: 12-hour, with AM and PM). */
    dateTime: string
    hours24: string
    /** Sound, and in it the Equalizer: the speakers as tuned (Recommended) or the player's own tone (Custom), what
     * it does, its three bands under the graph, and the curve set back. */
    sound: string
    equalizer: string
    recommended: string
    custom: string
    equalizerFooter: string
    low: string
    mid: string
    high: string
    resetEqualizer: string
    back: string
  }
  /** Apps. */
  apps: Record<AppId, string>
  /** An app not ready yet (Phone, Messages): what its window says. */
  notBuiltYet: string
  /** The Calendar app's day without events (Apple's words). */
  noEvents: string
  /** Player's words: the game's music folder without songs, and what to do. */
  player: {
    noSongs: string
    addSongs: string
    /** Now Playing's volume bar, and its speaker button (Apple Music's words). */
    volume: string
    mute: string
    unmute: string
  }
}

const STRINGS: Record<string, Strings> = Object.fromEntries(
  Object.entries(import.meta.glob<Strings>('./[a-z][a-z]_[a-z][a-z].ts', { eager: true, import: 'strings' })).map(
    ([path, strings]) => [path.slice(2, -3), strings],
  ),
)

/** English, for a language without its words. */
const FALLBACK = 'en_us'

/** The cabin's language for the game's (its `g_lang_init`, e.g. `tr_tr`; `null` until known): the game's
 * own when there are words in it, English otherwise. */
export function languageOf(gameLanguage: string | null): Language {
  const code = gameLanguage?.toLowerCase()
  return code && STRINGS[code] ? code : FALLBACK
}

/** The cabin's words in `language`. */
export function stringsFor(language: Language): Strings {
  return STRINGS[language] ?? STRINGS[FALLBACK]
}

/** The languages there are words for. */
export function cabinLanguages(): Language[] {
  return Object.keys(STRINGS).sort()
}
