/**
 * App icons by role. The UI only ever asks for a role (and a look, light or dark), so replacing the icon set
 * means changing this file.
 *
 * WestPlay's own icons (public/icons/westplay), each in a light and a dark look: drawn in the logo's style, and
 * for the music services their companies' own marks, from their official files (on their own background in the
 * light look, on WestPlay's dark card in the dark look).
 */
export type AppIconRole =
  | 'maps'
  | 'music'
  | 'spotify'
  | 'podcasts'
  | 'messages'
  | 'phone'
  | 'calendar'
  | 'settings'
  | 'weather'
  | 'youtube'
  | 'youtubeMusic'
  | 'player'
  | 'safari'

/** Each role's files' name. */
const FILES: Record<AppIconRole, string> = {
  maps: 'maps',
  music: 'music',
  spotify: 'spotify',
  podcasts: 'podcasts',
  messages: 'messages',
  phone: 'phone',
  calendar: 'calendar',
  settings: 'settings',
  weather: 'weather',
  youtube: 'youtube',
  youtubeMusic: 'youtube-music',
  player: 'player',
  safari: 'safari',
}

/** WestPlay's own icon for `role`, for a light or a dark screen. */
export function appIcon(role: AppIconRole, look: 'light' | 'dark' = 'light'): string {
  return `/icons/westplay/${FILES[role]}.${look}.svg`
}
