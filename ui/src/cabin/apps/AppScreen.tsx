import { useAppIcon } from '../state/hooks'
import { MapView } from '../dashboard/MapView'
import { type AppId, APPS } from '../state/apps'
import { useLaunchStyle } from '../state/launch'
import { useText } from '../state/text'
import { MusicApp } from './music/MusicApp'
import { SettingsApp } from './settings/SettingsApp'
import { SpotifyApp } from './spotify/SpotifyApp'
import { YouTubeMusicApp } from './youtubeMusic/YouTubeMusicApp'
import { WeatherApp } from './weather/WeatherApp'
import { YouTubeApp } from './youtube/YouTubeApp'
import './apps.css'

/**
 * An open app: its window, which grows out of the icon that opened it and shrinks back into its Home icon
 * (state/launch.ts). Like CarPlay's, app windows are opaque: the wallpaper belongs to Home and the
 * Dashboard. Each app replaces its placeholder as it is built (M4-M6).
 */
export function AppScreen({ app }: { app: AppId }) {
  const launch = useLaunchStyle(app, { kind: 'window' })
  return (
    <div className="screen app-window" style={launch}>
      <AppContent app={app} />
    </div>
  )
}

function AppContent({ app }: { app: AppId }) {
  const text = useText()
  switch (app) {
    case 'maps':
      return (
        <section className="app-panel maps-app" aria-label={text.apps.maps}>
          <MapView interactive />
        </section>
      )
    case 'music':
      return <MusicApp />
    case 'youtube':
      return <YouTubeApp />
    case 'spotify':
      return <SpotifyApp />
    case 'youtubeMusic':
      return <YouTubeMusicApp />
    case 'settings':
      return <SettingsApp />
    case 'weather':
      return <WeatherApp />
    default:
      return <NotBuiltYet app={app} />
  }
}

/**
 * DEVELOPMENT PLACEHOLDER for apps that are not built yet, so navigation can be tried end to end. It is
 * deliberately plain; no app ships with it.
 */
function NotBuiltYet({ app }: { app: AppId }) {
  const text = useText()
  const icon = useAppIcon(APPS[app].icon)
  return (
    <section className="app-panel not-built" aria-label={text.apps[app]}>
      <img src={icon} alt="" />
      <div className="name">{text.apps[app]}</div>
      <div className="note">{text.notBuiltYet}</div>
    </section>
  )
}
