/**
 * The companion's own words, past the onboarding: its sections, their pages, the tutorial and the uninstall,
 * in each of the game's 43 languages (by the game's codes, as the onboarding's words), in the game's language
 * as WestPlay speaks it in the cab; English for a language the game doesn't have. Each language says "you"
 * as its onboarding does (informally or not) and names things as it does (the truck, the game's Mod Manager).
 * What the onboarding already says (the keys, the sign-ins, the map's stages, the game's summary) is the
 * onboarding's words.
 *
 * The translations other than English and Turkish were written without a native speaker's review yet.
 */
export interface CompanionWords {
  sections: Record<SectionId, string>
  /** The list's line under WestPlay's name: where WestPlay is now. */
  state: {
    starting: string
    stopped: string
    noGame: string
    /** The player's truck isn't one WestPlay shows in. */
    unsupported: string
    notInCab: string
    live: string
    original: string
  }
  values: {
    on: string
    off: string
    ready: string
    waiting: string
    failed: string
    /** `{n}`: a percentage. */
    building: string
    signedIn: string
    none: string
    noGame: string
  }
  screen: {
    titles: {
      starting: string
      stopped: string
      noGame: string
      unsupported: string
      notInCab: string
      live: string
      original: string
    }
    subs: {
      starting: string
      stopped: string
      noGame: string
      unsupported: string
      notInCab: string
      live: string
      original: string
    }
    show: string
    showDetail: string
    connection: string
    engine: string
    engineRunning: string
    engineStarting: string
    game: string
    gameConnected: string
    gameNotRunning: string
    truckScreen: string
    /** `{fps}` */
    screenLive: string
    /** The truck screen's row, in a truck WestPlay doesn't show in. */
    screenUnsupported: string
    screenNotInView: string
    /** The graphics card the screen is drawn on (its group is titled with `about.gpu`): WestPlay's pick, or
     * one the player chose. */
    gpuAuto: string
    gpuAutoDetail: string
    gpuInUse: string
    gpuNote: string
    /** The game runs as administrator: WestPlay can't read its route, guidance or weather (owner 2026-10-28:
     * WestPlay doesn't reach into an elevated game; the player is told). */
    elevated: { title: string; text: string }
    banners: {
      live: { title: string; text: string }
      original: { title: string; text: string }
      noGame: { title: string; text: string }
      unsupported: { title: string; text: string }
      notInCab: { title: string; text: string }
    }
  }
  controls: {
    title: string
    sub: string
    key: string
    pointer: string
    speed: string
    speedDetail: string
    strip: string
    stripDetail: string
    /** The key's title in toggle mode (`key`: held). */
    keyToggle: string
    keybinds: string
    /** The screen's key, as a keybind (and what a key refused is taken for). */
    useScreen: string
    toggle: string
    /** `{key}`: the screen's key. */
    toggleDetail: string
    volumeUp: string
    volumeDown: string
    playPause: string
    /** A keybind listening for its new key. */
    pressKey: string
    /** `{key}`: the screen's key. */
    keybindsNote: string
    /** `{action}`: what the key is taken for. */
    inUse: string
  }
  accounts: {
    title: string
    sub: string
  }
  map: {
    titles: { ready: string; building: string; waiting: string; failed: string; unknown: string; noGame: string }
    sub: string
    /** The button that prepares every profile's map again. */
    prepareAgain: string
    inCab: string
    /** `{date}` */
    built: string
    notBuilt: string
    profiles: string
    checking: string
    /** Waiting for memory to be free: the page's title, the screen's banner, and the note (`{needed}`,
     * `{free}`: gigabytes). */
    memory: { title: string; banner: { title: string; text: string }; note: { title: string; text: string } }
  }
  setup: {
    title: string
    sub: string
    game: string
    folder: string
    installed: string
    plugin: string
    pluginStates: { current: string; different: string; missing: string }
    reinstall: string
    tutorial: string
    /** The banner's title; its line is what the plugin does (the onboarding's `pluginDetail`). */
    reinstalled: string
    closeGame: { title: string; text: string }
    checking: { title: string; text: string }
    checked: { title: string; text: string }
  }
  uninstall: UninstallWords
  tutorial: TutorialWords
  about: {
    title: string
    sub: string
    version: string
    diagnostics: string
    frames: string
    /** `{n}` */
    perSecond: string
    dropped: string
    gpu: string
    cabGpu: string
    gameGpu: string
    webview: string
    waitingForGame: string
    log: string
    /** The app's own log (app.log), with where it is: Documents, or WestPlay's folder where Documents refused it. */
    appLog: string
    copy: string
    copied: { title: string; text: string }
  }
  /** Customize: the player's own wallpapers and app icons. `{n}`: a count; `{names}`: names, listed. */
  customize: {
    title: string
    sub: string
    wallpapers: string
    add: string
    pair: string
    wallpapersNote: string
    icons: string
    westplay: string
    iconCount: string
    addFolder: string
    addZip: string
    iconsNote: string
    looksNote: string
    style: { title: string; auto: string; light: string; dark: string }
    remove: string
    pickWallpapers: string
    pickWallpaperFolder: string
    pickFolder: string
    pickZip: string
    images: string
    zips: string
    added: string
    nothingAdded: string
    problems: { notAnImage: string; tooLarge: string; unknownName: string; twice: string; unreadable: string }
  }
  /** WestPlay's updates: the About page's row, the list's note, the sheet offering one, and finishing one
   * (its plugin and mod into the game). `{version}`: the update's. */
  update: {
    row: string
    upToDate: string
    available: string
    badge: string
    title: string
    sub: string
    whatsNew: string
    later: string
    install: string
    closeGame: { title: string; text: string }
    downloading: string
    installing: string
    failed: string
    banners: { updating: { title: string; text: string }; updated: { title: string; text: string } }
    finish: { title: string; text: string; button: string }
  }
  /** Support me: WestPlay's development, supported on its maker's Kreosus page (opened in the browser). */
  support: {
    title: string
    sub: string
    button: string
    note: string
  }
}

export interface UninstallWords {
  row: string
  title: string
  text: string
  keep: string
  keepDetail: string
  cancel: string
  confirm: string
  asAdministrator: string
  closeGame: { title: string; text: string }
  steps: { profiles: string; plugin: string; data: string }
  /** `{n}`: how many. */
  profilesFailed: string
  banners: { working: { title: string; text: string }; done: { title: string; text: string } }
  doneTitle: string
  doneSub: string
}

/** A tutorial step: its words under the screen, and its banner on it. `{key}`: the key the player holds. */
export interface Lesson {
  title: string
  sub: string
  banner: { title: string; text: string }
}

export interface TutorialWords {
  intro: { title: string; sub: string; start: string; skip: string }
  hold: Lesson
  open: Lesson
  home: Lesson
  dashboard: Lesson
  letGo: Lesson
  done: Lesson
  /** The key let go too soon. */
  holdAgain: { title: string; text: string }
  next: string
  /** Each section, as the tour shows it. */
  tour: Record<SectionId, string>
  finish: string
}

export type SectionId = 'screen' | 'controls' | 'customize' | 'accounts' | 'map' | 'setup' | 'about' | 'support'

const WORDS: Record<string, CompanionWords> = Object.fromEntries(
  Object.entries(import.meta.glob<CompanionWords>('./[a-z][a-z]_[a-z][a-z].ts', { eager: true, import: 'words' })).map(
    ([path, words]) => [path.slice(2, -3), words],
  ),
)

/** The companion's words in the game's language `code` (`tr_tr`), English for one without. */
export function companionWords(code: string): CompanionWords {
  return WORDS[code] ?? WORDS.en_us
}

export function companionLanguages(): string[] {
  return Object.keys(WORDS).sort()
}
