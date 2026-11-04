import type { AppIconRole } from '../../shared/icons'

/** Every app WestPlay knows, in Home grid order. */
export const APP_IDS = [
  'maps',
  'music',
  'spotify',
  'youtube',
  'youtubeMusic',
  'player',
  'podcasts',
  'phone',
  'messages',
  'calendar',
  'weather',
  'settings',
] as const

export type AppId = (typeof APP_IDS)[number]

/** An app: its id and icon (its name is in the cabin's language: state/text.ts). */
export interface AppDefinition {
  id: AppId
  icon: AppIconRole
}

export const APPS: Record<AppId, AppDefinition> = {
  maps: { id: 'maps', icon: 'maps' },
  music: { id: 'music', icon: 'music' },
  spotify: { id: 'spotify', icon: 'spotify' },
  youtube: { id: 'youtube', icon: 'youtube' },
  youtubeMusic: { id: 'youtubeMusic', icon: 'youtubeMusic' },
  player: { id: 'player', icon: 'player' },
  podcasts: { id: 'podcasts', icon: 'podcasts' },
  phone: { id: 'phone', icon: 'phone' },
  messages: { id: 'messages', icon: 'messages' },
  calendar: { id: 'calendar', icon: 'calendar' },
  weather: { id: 'weather', icon: 'weather' },
  settings: { id: 'settings', icon: 'settings' },
}

export function isAppId(value: unknown): value is AppId {
  return typeof value === 'string' && (APP_IDS as readonly string[]).includes(value)
}

/** Not in this release: Podcasts. Its id stays (its icon, players' icon sets), but it's not on Home, and a saved
 * screen or recent can't open it. (Phone and Messages are on Home, their windows saying they aren't ready yet:
 * owner 2026-11-03.) */
const NEXT_RELEASE: ReadonlySet<AppId> = new Set<AppId>(['podcasts'])

/** The apps this release has, in Home grid order. */
export const RELEASED_APP_IDS: readonly AppId[] = APP_IDS.filter((id) => !NEXT_RELEASE.has(id))

/** An app this release has (`isAppId`, less the ones still to come). */
export function isReleasedApp(value: unknown): value is AppId {
  return isAppId(value) && !NEXT_RELEASE.has(value)
}
