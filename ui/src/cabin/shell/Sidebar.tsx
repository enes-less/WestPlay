import { formatGameTime } from '../../shared/format'
import { useAppIcon, useClockStyle, useScreenProfile } from '../state/hooks'
import { type AppId, APPS } from '../state/apps'
import { LAUNCH_STYLE, useCarriesLaunch } from '../state/launch'
import { homeButtonTarget, sidebarApps } from '../state/navigation'
import { go, useCabin, useLive } from '../state/store'
import { useText } from '../state/text'
import { cabClock } from '../../shared/host'

/**
 * The CarPlay sidebar: clock and signal, an app of each kind (the last used maps app, music app...:
 * `sidebarApps`), and at the bottom the truck button (back to the truck's own screen, like CarPlay's car button)
 * and the Home button.
 */
export function Sidebar() {
  const recents = useCabin((state) => state.navigation.recents)
  const route = useCabin((state) => state.navigation.route)
  const homeTarget = useCabin((state) => homeButtonTarget(state.navigation))
  const gameTime = useLive((state) => (state.telemetry ? cabClock(state.telemetry, state.clockShift) : null))
  const apps = sidebarApps(recents, useScreenProfile().recents ?? 3)
  const clock = useClockStyle(false)
  const tablet = useLive((state) => state.screenLayout === 'tablet')

  return (
    // Its own transition name: it stays put while screens change (shell/transitions.css).
    <aside className="sidebar glass" style={{ viewTransitionName: 'sidebar' }}>
      <div className="status">
        <div className="time">{gameTime === null ? '--:--' : formatGameTime(gameTime, clock)}</div>
        {/* Placeholder until the phone remote (planned) reports the real phone. */}
        <div className="signal">
          <SignalBars /> 5G
        </div>
      </div>
      <nav className="recent" aria-label="Apps">
        {apps.map((app) => (
          <RecentApp key={app} app={app} current={route.kind === 'app' && route.app === app} />
        ))}
      </nav>
      <div className="sidebar-actions">
        {/* Not on WestPlay's tablet: it's no truck's screen, nothing to hand back to (owner 2026-11-11). */}
        {!tablet && (
          <button
            type="button"
            className="sidebar-button truck-button"
            aria-label="Truck screen"
            onClick={() => useCabin.getState().setTruckScreen(true)}
          >
            <TruckGlyph />
          </button>
        )}
        {/* Its icon is where it goes: the app grid (Home) or the Dashboard. */}
        <button
          type="button"
          className="sidebar-button home-button"
          aria-label={homeTarget === 'home' ? 'Home' : 'Dashboard'}
          onClick={() => go({ type: 'pressHome' })}
        >
          <GridGlyph shown={homeTarget === 'home'} />
          <DashboardGlyph shown={homeTarget === 'dashboard'} />
        </button>
      </div>
    </aside>
  )
}

/**
 * A sidebar app. Its icon has a transition name of its own, for the window growing out of it and shrinking back
 * into it. When it opens its app, the window grows out of a copy laid exactly over it for that moment.
 */
function RecentApp({ app, current }: { app: AppId; current: boolean }) {
  const launching = useCarriesLaunch(app, { kind: 'icon', origin: 'sidebar' })
  const icon = useAppIcon(APPS[app].icon)
  const text = useText()
  return (
    <button
      type="button"
      aria-current={current ? 'page' : undefined}
      aria-label={text.apps[app]}
      onClick={() => go({ type: 'openApp', app }, 'sidebar')}
    >
      <img src={icon} alt="" style={{ viewTransitionName: `recent-${app}`, viewTransitionClass: 'recent' }} />
      {launching && <img className="launch-copy" src={icon} alt="" style={LAUNCH_STYLE} />}
    </button>
  )
}

/** Home: the app grid. */
function GridGlyph({ shown }: { shown: boolean }) {
  return (
    <svg viewBox="0 0 24 24" width="34" height="34" className={shown ? 'shown' : undefined} aria-hidden>
      {[4, 10.25, 16.5].flatMap((x) =>
        [4, 10.25, 16.5].map((y) => (
          <rect key={`${x}-${y}`} x={x} y={y} width="3.5" height="3.5" rx="1" fill="currentColor" />
        )),
      )}
    </svg>
  )
}

/**
 * The Dashboard: one rounded square, outlined, as CarPlay's (owner 2026-11-11, from their photo of it; it had been
 * one large tile and two small ones). As large as the app grid it trades places with (`GridGlyph`: 16 of its 24
 * units, at the same size, so it grows and shrinks with the sidebar as the grid does); drawn as in the photo, read
 * from its pixels, for its side of 45 there: the line 5 thick, the outside's corners rounded 10.5 and the inside's 5.5,
 * on the same centres.
 */
function DashboardGlyph({ shown }: { shown: boolean }) {
  // The photo's 45 pixels to the grid's 16 units.
  const unit = 16 / 45
  const line = 5 * unit
  return (
    <svg
      viewBox="0 0 24 24"
      width="34"
      height="34"
      className={shown ? 'shown' : undefined}
      fill="none"
      stroke="currentColor"
      strokeWidth={line}
      aria-hidden
    >
      <rect x={4 + line / 2} y={4 + line / 2} width={16 - line} height={16 - line} rx={(10.5 - 2.5) * unit} />
    </svg>
  )
}

/** Back to the truck: the front of a truck cab (our own glyph, not CarPlay's car). */
function TruckGlyph() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="36"
      height="36"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinejoin="round"
      strokeLinecap="round"
      aria-hidden
    >
      <path d="M6.5 3.5h11a2 2 0 0 1 2 2V17h-15V5.5a2 2 0 0 1 2-2Z" />
      <path d="M7 6.5h10v4.5H7Z" />
      <path d="M7.5 14h1.5M15 14h1.5" />
      <path d="M6 17v2.5h2.5V17M15.5 17v2.5H18V17" />
    </svg>
  )
}

function SignalBars() {
  return (
    <svg viewBox="0 0 20 14" width="26" height="18" aria-hidden>
      {[0, 1, 2, 3].map((bar) => (
        <rect
          key={bar}
          x={bar * 5.5}
          y={10 - bar * 3.3}
          width="3.6"
          height={4 + bar * 3.3}
          rx="1"
          fill="currentColor"
        />
      ))}
    </svg>
  )
}
