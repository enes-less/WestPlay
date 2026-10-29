import { startTransition, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { useAppIcon } from '../state/hooks'
import { MapView } from '../dashboard/MapView'
import { type AppId, APPS } from '../state/apps'
import { useLaunchStyle, whenTransitionEnds } from '../state/launch'
import { useLanguage, useText } from '../state/text'
import { isCopied, noteOpened, openedBefore, saveCopy, savedCopy } from './launchCopies'
import { MusicApp } from './music/MusicApp'
import { SettingsApp } from './settings/SettingsApp'
import { SpotifyApp } from './spotify/SpotifyApp'
import { YouTubeMusicApp } from './youtubeMusic/YouTubeMusicApp'
import { WeatherApp } from './weather/WeatherApp'
import { YouTubeApp } from './youtube/YouTubeApp'
import './apps.css'
import { LocalPlayerApp } from './localPlayer/LocalPlayerApp'

/**
 * An open app: its window, which grows out of the icon that opened it and shrinks back into its Home icon
 * (state/launch.ts). Like CarPlay's, app windows are opaque: the wallpaper belongs to Home and the
 * Dashboard. Each app replaces its placeholder as it is built (M4-M6).
 *
 * A heavy app the player opens for the first time since the page loaded (apps/launchCopies.ts) first shows
 * its saved launch copy, static: the window
 * grows out of the icon with that, nothing to build in the frame it starts. Once the window has opened, the
 * real app is built (in slices) and takes the copy's place; its first frame, as the player sees it (signed
 * in or not, their language and appearance), is saved as the copy for the next opening.
 */
export function AppScreen({ app }: { app: AppId }) {
  const launch = useLaunchStyle(app, { kind: 'window' })
  const language = useLanguage()
  // Its first opening since the page loaded: the cold one, through the copy.
  const [first] = useState(() => isCopied(app) && !openedBefore(app))
  const [copy] = useState(() => (first ? savedCopy(app, language) : null))
  const [live, setLive] = useState(copy === null)
  // The real app's first frame at a first opening (cold: its own loading looks, signed in or not), saved as
  // the next first opening's copy, before it's drawn: what's saved is what replaces the copy. (A copy built
  // apart showed the signed-out app; one saved at a later opening held its content: owner 2026-10-21.)
  const appWindow = useRef<HTMLDivElement>(null)
  const saved = useRef(false)
  useLayoutEffect(() => {
    if (!live || saved.current || !first) return
    saved.current = true
    noteOpened(app)
    const markup = appWindow.current?.innerHTML
    if (markup) saveCopy(app, language, markup)
  }, [live, first, app, language])
  useEffect(() => {
    if (live) return
    let gone = false
    whenTransitionEnds(() => {
      if (!gone) startTransition(() => setLive(true))
    })
    return () => {
      gone = true
    }
  }, [live])
  if (!live && copy !== null) {
    // The copy: markup this page saved from its own app, shown as it was (no script in it).
    return (
      <div ref={appWindow} className="screen app-window" style={launch} dangerouslySetInnerHTML={{ __html: copy }} />
    )
  }
  return (
    <div ref={appWindow} className="screen app-window" style={launch}>
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
    case 'localPlayer':
      return <LocalPlayerApp />
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
