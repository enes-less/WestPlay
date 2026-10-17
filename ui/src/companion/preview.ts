import type { GameCheck, GameInfo, InstallReport, SetupState } from './setup'
import type { Customizations } from '../shared/customize'
import type { Added } from './customize'
import type { Offer } from './update'
import type { HostStatus, MapStatus } from './useHostStatus'
import type { Settings } from './useSettings'

/**
 * The companion opened in a plain browser (bridge.ts): the app's answers, made up, so its pages can be
 * developed and checked without the app or the game. What it shows is set in the address:
 *
 * - `?onboarding`: the first run, from its start.
 * - `?game=off|on|live`: the game not running, running (the cab not on the screen), or the screen live.
 * - `?map=building|ready|waiting|failed`: the map's state (`building` runs to `ready`).
 * - `?update`: an update found (its sheet offers it; installing it downloads, then stays installing).
 * - `?custom`: some wallpapers and an icon set of the player's (adding gives more, with a file left out).
 * - `?updated` (or `?updated=admin`): WestPlay updated since it was set up: the new plugin goes into the game
 *   once it's closed (with `admin`, Windows' permission is asked for first).
 *
 * Nothing here is ever in the app.
 */
const query = new URLSearchParams(location.search)
const gameState = query.get('game') ?? 'live'

const GAME: GameInfo = {
  dir: 'C:\\Program Files (x86)\\Steam\\steamapps\\common\\Euro Truck Simulator 2',
  foundBy: 'steam',
  version: '1.61.1.9',
  tested: true,
  dlcs: 34,
}

// A first run has no game set up until the onboarding installs into it.
const setup: SetupState = {
  finished: !query.has('onboarding'),
  game: query.has('onboarding') ? null : GAME,
  found: [GAME],
  updated: query.has('updated'),
  gameRunning: gameState !== 'off',
  language: query.get('lang'),
}

const UPDATE: Offer | null = query.has('update')
  ? {
      version: '1.1.0',
      notes: [
        '- Song lyrics on the Now Playing screen.',
        '- The map follows the route around roundabouts.',
        '- **Fixed:** YouTube Music skipping the first song.',
      ].join('\n'),
      date: '2026-11-02',
    }
  : null

let settings: Settings = {
  interaction: { key: 'leftAlt', sensitivity: 1.4 },
  screen: { truckStrip: 'auto' },
  look: { wallpaper: null, iconSet: null, iconStyle: 'auto' },
}

// The player's own, made up from WestPlay's files (the preview's address: `FILES_BASE` is `/` here).
const custom: Customizations = query.has('custom')
  ? {
      wallpapers: [{ id: 'custom:Logo', name: 'Logo', dark: 'westplay.png', light: null }],
      iconSets: [
        {
          name: 'Sample icons',
          icons: {
            spotify: { light: 'icons/westplay/spotify.dark.svg', dark: 'icons/westplay/spotify.dark.svg' },
            youtube: { light: null, dark: 'icons/westplay/youtube.light.svg' },
          },
        },
      ],
    }
  : { wallpapers: [], iconSets: [] }

const accounts = {
  youtube: true as boolean | null,
  spotify: false as boolean | null,
  appleMusic: false as boolean | null,
  signingIn: '',
}

function startingMap(): MapStatus {
  switch (query.get('map')) {
    case 'building':
      return { state: 'building', done: 0.05, stage: 'reading' }
    case 'waiting':
      return { state: 'waitingForGame' }
    case 'memory':
      return { state: 'waitingForMemory', needed: 3_500_000_000, free: 2_140_000_000 }
    case 'failed':
      return { state: 'failed', error: 'The map couldn’t be read: def/world/road_look.sii is missing.' }
    default:
      return { state: 'ready' }
  }
}
let map = startingMap()
let westplayOpen: boolean | null = gameState === 'live' ? true : null

function status(): HostStatus {
  const live = gameState === 'live'
  return {
    webviewVersion: '141.0.3537.71',
    adapter: gameState === 'off' ? null : 'NVIDIA GeForce RTX 4070',
    framesPerSecond: live ? 58 + Math.round(Math.random() * 3) : 0,
    droppedTotal: live ? 3 : 0,
    gameConnected: gameState !== 'off',
    screenDrawing: live,
    westplayOpen: live ? westplayOpen : null,
    youtubeSignedIn: accounts.youtube,
    youtubeSigningIn: accounts.signingIn === 'youtube',
    spotifySignedIn: accounts.spotify,
    spotifySigningIn: accounts.signingIn === 'spotify',
    appleMusicSignedIn: accounts.appleMusic,
    appleMusicSigningIn: accounts.signingIn === 'appleMusic',
    map,
    mapFolder: null,
    mapBuilt: map.state === 'ready' ? Math.floor(Date.now() / 1000) - 3 * 86_400 : null,
    error: null,
  }
}

const listeners = new Map<string, Set<(payload: unknown) => void>>()
const emit = (event: string, payload: unknown) => listeners.get(event)?.forEach((handler) => handler(payload))

// The status, about once a second, as the app sends it; a map being built goes on.
const STAGES = ['reading', 'drawing', 'tiles', 'saving'] as const
const ENDS = [0.15, 0.6, 0.95, 1]
setInterval(() => {
  if (map.state === 'building') {
    const done = Math.min(1, map.done + 0.012)
    const stage = STAGES[ENDS.findIndex((end) => done < end)] ?? 'saving'
    map = done >= 1 ? { state: 'ready' } : { state: 'building', done, stage }
  }
  emit('host-status', status())
}, 1000)

export function subscribe(event: string, handler: (payload: unknown) => void): () => void {
  const set = listeners.get(event) ?? new Set()
  set.add(handler)
  listeners.set(event, set)
  if (event === 'host-status') setTimeout(() => handler(status()), 50)
  return () => set.delete(handler)
}

function setLook(change: Partial<Settings['look']>) {
  settings = { ...settings, look: { ...settings.look, ...change } }
  emit('settings-changed', settings)
}

const later = <T>(value: T, ms = 300) => new Promise<T>((resolve) => setTimeout(() => resolve(value), ms))

/** A sign-in: Google's, Spotify's or Apple's window "open" a moment, then signed in. */
function signIn(service: 'youtube' | 'spotify' | 'appleMusic') {
  accounts.signingIn = service
  emit('host-status', status())
  setTimeout(() => {
    accounts[service] = true
    accounts.signingIn = ''
    emit('host-status', status())
  }, 2500)
}

const CHECK: GameCheck = {
  game: GAME,
  dlcs: 34,
  mapDlcs: ['Going East!', 'Scandinavia', 'Vive la France!', 'Italia', 'Beyond the Baltic Sea', 'Iberia'],
  profiles: [
    {
      name: 'Western',
      map: 'europe',
      mapMods: ['ProMods 2.80'],
      unreadable: 0,
      hvtMap: false,
      westplayMod: true,
    },
    { name: 'Test drive', map: 'europe', mapMods: [], unreadable: 0, hvtMap: false, westplayMod: true },
  ],
  trucks: [
    { id: 'daf-xf-2021', name: 'DAF XF 2021', fit: 'ready', fromMod: false },
    { id: 'scania-smart-dash', name: 'Scania S and R - Smart Dash', fit: 'ready', fromMod: false },
    { id: 'iveco-sway', name: 'Iveco S-Way', fit: 'ready', fromMod: false },
    { id: 'renault-t-2024', name: 'Renault T (2024)', fit: 'ready', fromMod: false },
  ],
  modProblem: null,
  plugin: 'current',
}

export function answer(command: string, args?: Record<string, unknown>): Promise<unknown> {
  switch (command) {
    case 'setup_state':
      return later(setup, 120)
    case 'game_running':
      return later(gameState !== 'off', 80)
    case 'check_game':
      return later(CHECK, 900)
    case 'choose_game_folder':
      return later(null)
    case 'install_into_game':
      if (query.get('updated') === 'admin' && !args?.administrator) {
        return later<InstallReport>({ plugin: { state: 'needsAdministrator' }, modError: null, done: false }, 900)
      }
      setup.game = GAME
      return later<InstallReport>({ plugin: { state: 'installed' }, modError: null, done: true }, 1600)
    case 'finish_setup':
      setup.finished = true
      return later(undefined)
    case 'get_settings':
      return later(settings, 60)
    case 'update_settings':
      settings = args?.settings as Settings
      emit('settings-changed', settings)
      return later(settings, 60)
    case 'set_westplay_open':
      westplayOpen = Boolean(args?.open)
      return later(undefined, 200)
    case 'youtube_check':
    case 'spotify_check':
    case 'apple_music_check':
      return later(undefined)
    case 'youtube_sign_in':
      signIn('youtube')
      return later(undefined)
    case 'spotify_sign_in':
      signIn('spotify')
      return later(undefined)
    case 'apple_music_sign_in':
      signIn('appleMusic')
      return later(undefined)
    case 'open_support_page':
      window.open('https://kreosus.com/enes-less', '_blank', 'noopener')
      return later(undefined)
    case 'youtube_sign_out':
      setTimeout(() => {
        accounts.youtube = false
        emit('host-status', status())
      }, 1200)
      return later(undefined)
    case 'spotify_sign_out':
      setTimeout(() => {
        accounts.spotify = false
        emit('host-status', status())
      }, 1200)
      return later(undefined)
    case 'apple_music_sign_out':
      setTimeout(() => {
        accounts.appleMusic = false
        emit('host-status', status())
      }, 1200)
      return later(undefined)
    case 'uninstall_from_game':
      return later(
        { plugin: { state: 'installed' }, modError: null, profilesChanged: 1, profilesFailed: 0, done: true },
        1400,
      )
    case 'remove_data':
      // The maps and the sign-ins going take a while: the red button turns meanwhile.
      return later(undefined, 2500)
    case 'leave_pc':
      // The app would quit: the preview starts over, at the onboarding.
      setTimeout(() => location.assign('/?onboarding'), 400)
      return later(undefined)
    case 'companion_ready':
      return later(undefined, 0)
    case 'customize_list':
      return later(structuredClone(custom), 150)
    case 'add_wallpapers': {
      // A folder or pictures: the same here.
      // The picker closes, then the files take a while.
      setTimeout(() => emit('customize-adding', null), 400)
      const name = `Sample ${custom.wallpapers.length + 1}`
      custom.wallpapers.push({ id: `custom:${name}`, name, dark: 'westplay.png', light: null })
      return later<Added>({ added: [name], problems: [{ file: 'notes.txt', reason: 'notAnImage' }] }, 2400)
    }
    case 'add_icon_set': {
      setTimeout(() => emit('customize-adding', null), 400)
      const name = `Icons ${custom.iconSets.length + 1}`
      const music = 'icons/westplay/music.dark.svg'
      custom.iconSets.push({ name, icons: { music: { light: music, dark: music } } })
      return later<Added>({ added: [name], problems: [{ file: 'Spotify Logo.png', reason: 'unknownName' }] }, 2400)
    }
    case 'remove_wallpaper':
      custom.wallpapers = custom.wallpapers.filter((wallpaper) => wallpaper.id !== args?.id)
      if (settings.look.wallpaper === args?.id) setLook({ wallpaper: null })
      return later(undefined)
    case 'remove_icon_set':
      custom.iconSets = custom.iconSets.filter((set) => set.name !== args?.name)
      if (settings.look.iconSet === args?.name) setLook({ iconSet: null })
      return later(undefined)
    case 'update_offer':
      return later(UPDATE)
    case 'look_for_update':
      if (UPDATE) emit('update-found', UPDATE)
      return later(UPDATE, 700)
    case 'install_update': {
      const total = 48_000_000
      for (let step = 1; step <= 20; step++) {
        setTimeout(() => emit('update-progress', { done: (total * step) / 20, total }), step * 150)
      }
      // The app quits for the installer: nothing comes back.
      return new Promise(() => {})
    }
    default:
      return Promise.reject(new Error(`preview: no answer for ${command}`))
  }
}
