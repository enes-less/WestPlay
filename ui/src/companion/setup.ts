import { call } from './bridge'
import type { HostStatus } from './useHostStatus'

/** An install of the game. Mirrors the app's `setup::GameInfo`. */
export interface GameInfo {
  dir: string
  foundBy: 'steam' | 'installed' | 'chosen'
  version: string | null
  /** WestPlay has been tried with this version. */
  tested: boolean
  dlcs: number
}

/** Mirrors the app's `setup::SetupState`. */
export interface SetupState {
  /** The onboarding was gone through: the companion opens on its own pages. */
  finished: boolean
  /** The game WestPlay is set up in, if it is. */
  game: GameInfo | null
  /** WestPlay was updated since it put its plugin and mod into that game: the new ones go in once it's
   * closed (`FinishUpdate`). */
  updated: boolean
  /** Every install of the game found on the PC. */
  found: GameInfo[]
  gameRunning: boolean
  /** The onboarding's language, a game language code (`tr_tr`): the game's own (or one set to try it out,
   * `WESTPLAY_LANGUAGE`); `null` before the game was ever started: then Windows'. */
  language: string | null
}

/** What a check of the game found. Mirrors the app's `setup::GameCheck`. */
export interface GameCheck {
  game: GameInfo
  /** How many DLCs the game has (one per Steam DLC: paint jobs and flags packs count too). */
  dlcs: number
  /** Those that change the map, by name. */
  mapDlcs: string[]
  /** The most recently saved first. */
  profiles: ProfileCheck[]
  /** The trucks WestPlay is made for, and whether the player can have it in each. */
  trucks: TruckCheck[]
  /** Why WestPlay's mod can't be built from this game, if it can't. */
  modProblem: string | null
  plugin: 'missing' | 'current' | 'different'
  /** Other CarPlay apps installed in the game, which take the trucks' screens: to be removed. */
  otherApps: 'ymnOne'[]
}

export interface TruckCheck {
  /** `scania-smart-dash`: the names with words in them are put in the player's language. */
  id: string
  /** In English. */
  name: string
  /** `notYet`: in the game, but WestPlay doesn't draw on its screen yet. */
  fit: 'ready' | 'notInGame' | 'notYet'
  /** A mod's truck, not the game's. */
  fromMod: boolean
}

export interface ProfileCheck {
  name: string
  /** Its folder: the host's state of its map names it (`ProfileMap.dir`). */
  dir: string
  /** `europe`, or a standalone map mod's. */
  map: string
  /** The mods that change its map, as the game names them. */
  mapMods: string[]
  /** Map mods WestPlay couldn't read. */
  unreadable: number
  /** It plays HVT's map, which WestPlay doesn't allow. */
  hvtMap: boolean
}

/** Mirrors the app's `setup::PluginOutcome`. */
export type PluginOutcome =
  | { state: 'installed' }
  | { state: 'inUse' }
  | { state: 'needsAdministrator' }
  | { state: 'declined' }
  | { state: 'failed'; error: string }

/** Mirrors the app's `setup::InstallReport`. */
export interface InstallReport {
  plugin: PluginOutcome
  modError: string | null
  /** WestPlay is set up in this game: its map is built next. */
  done: boolean
}

export const setupState = () => call<SetupState>('setup_state')
export const gameRunning = () => call<boolean>('game_running')
/** Every profile's map prepared again, one by one (failed ones too); the cab keeps its map meanwhile. */
export const prepareMapsAgain = () => call<void>('prepare_maps_again')
/** Windows' folder picker: the game there, `null` if the player cancelled; rejects for a folder that isn't it. */
export const chooseGameFolder = (title: string) => call<GameInfo | null>('choose_game_folder', { title })
export const checkGame = (dir: string) => call<GameCheck>('check_game', { dir })
export const installIntoGame = (dir: string, administrator: boolean) =>
  call<InstallReport>('install_into_game', { dir, administrator })
export const finishSetup = () => call<void>('finish_setup')

/** Where the companion serves the player's maps (app/src/main.rs, `westplay-maps`). */
const MAPS = 'http://westplay-maps.localhost'

export function defaultMapAddress(status: HostStatus | null): { base: string; built: number | null } | null {
  return status?.mapFolder ? { base: `${MAPS}/${encodeURIComponent(status.mapFolder)}`, built: status.mapBuilt } : null
}

/** What taking WestPlay out of the game did. Mirrors the app's `setup::UninstallReport`; its `plugin` is
 * `installed` once the plugin is gone (the install's outcomes, the other way round). */
export interface UninstallReport {
  plugin: PluginOutcome
  modError: string | null
  profilesChanged: number
  profilesFailed: number
  /** WestPlay is out of the game: it can leave the PC next. */
  done: boolean
}

/** Takes WestPlay out of the game (its profiles' mods, its screens, the plugin), the game closed. */
export const uninstallFromGame = (administrator: boolean) =>
  call<UninstallReport>('uninstall_from_game', { administrator })
/** WestPlay's data goes while the uninstall shows it going (it can take a while): its engine stops, then the
 * sign-ins, and unless `keep`, the maps, the player's own looks and the settings. */
export const removeData = (keep: boolean) => call<void>('remove_data', { keep })
/** WestPlay leaves the PC (what was in use until it ends, and the app if it was installed), keeping settings
 * and maps if `keep`, and quits. */
export const leavePc = (keep: boolean) => call<void>('leave_pc', { keep })
