import { type CSSProperties, type ReactNode, useEffect, useLayoutEffect, useMemo, useRef } from 'react'
import { isInHost, postToHost } from '../../shared/host'
import { cursorArea } from './cursorArea'
import { useLook, useLookChanges, useOemStrip, useScreenProfile } from '../state/hooks'
import { usePreparedIcons } from '../state/looks'
import { frameInsets, NO_INSETS, OEM_STRIPS } from '../state/oem'
import { RETURN_BUTTON_SIZE, SIDEBAR_BUTTONS, returnButtonCentre } from '../state/screen'
import { textScale } from '../state/textSize'
import { Player } from '../apps/youtube/Player'
import { holeMask, onlyHole, usePageHole } from '../apps/safari/hole'
import { useYouTube } from '../apps/youtube/youtube'
import { useYouTubeMusic } from '../apps/youtubeMusic/youtubeMusic'
import { Keyboard } from '../keyboard/Keyboard'
import { useCabin, useLive } from '../state/store'
import { Sidebar } from './Sidebar'
import { useSidebarPeek } from './sidebarPeek'
import { VolumeHud } from './VolumeHud'
import './transitions.css'
import { Wallpaper } from './Wallpaper'

interface ShellProps {
  /** The current screen, placed in the CarPlay area. */
  children: ReactNode
  /** Full-screen content above everything, the OEM strip included (the boot sequence). */
  overlay?: ReactNode
  /** The first moments after the boot sequence: the screen fades in. */
  entering?: boolean
}

/**
 * The frame every screen lives in: the CarPlay area (wallpaper, sidebar and the current screen) and, in the
 * Glass style, room for the truck's own OEM strip beside it. Everything outside the CarPlay area is
 * transparent: the game draws the truck's real strip there, underneath.
 */
export function Shell({ children, overlay, entering = false }: ShellProps) {
  useLookChanges()
  usePreparedIcons()
  const { appearance, wallpaperSrc } = useLook()
  const profile = useScreenProfile()
  const stripId = useOemStrip()
  const strip = stripId && OEM_STRIPS[stripId]
  const layout = useLive((state) => state.screenLayout)
  const insets = strip ? frameInsets(strip.insets, layout) : NO_INSETS
  const style = {
    '--oem-left': `${insets.left}px`,
    '--oem-bottom': `${insets.bottom}px`,
    '--ui-scale': profile.scale,
    '--sidebar-icon-scale': profile.sidebarIcons ?? 1,
    '--sidebar-button-scale': profile.sidebarButtons ?? SIDEBAR_BUTTONS,
    // None: the stylesheet's own.
    '--screen-app-gap': px(profile.sidebarGaps?.apps),
    '--screen-home-bottom': px(profile.sidebarGaps?.homeBottom),
    '--screen-button-gap': px(profile.sidebarGaps?.buttons),
  } as CSSProperties
  // The screen changes' snapshots hang off <html>, not the shell: they're cut to the CarPlay area with these
  // too (transitions.css).
  useLayoutEffect(() => {
    const root = document.documentElement.style
    root.setProperty('--oem-left', `${insets.left}px`)
    root.setProperty('--oem-bottom', `${insets.bottom}px`)
  }, [insets.left, insets.bottom])
  // The profile's margin round CarPlay, on <html> too: where the screens start (`--content-left`) is worked out
  // there from it. None: the stylesheet's own.
  useLayoutEffect(() => {
    const root = document.documentElement.style
    if (profile.edge === undefined) root.removeProperty('--edge')
    else root.setProperty('--edge', `${profile.edge}px`)
  }, [profile.edge])
  // The apps' text size (state/textSize.ts), on <html> so menus portalled out of the apps and the screen
  // changes' snapshots take it too.
  const textSize = useCabin((state) => state.textSize)
  useLayoutEffect(() => {
    document.documentElement.style.setProperty('--text-scale', String(textScale(textSize)))
  }, [textSize])

  // Handed to the truck (the sidebar's truck button, or the W shortcut): CarPlay shrinks into WestPlay's
  // button and the truck's own screen shows; the button grows it back.
  const truckScreen = useCabin((state) => state.truckScreen)
  const button = returnButtonCentre(profile, insets.left, layout)
  // A video full screen: the rest of the CarPlay area steps aside for it (apps/youtube/youtube.css). A video
  // in small (YouTube's Now playing): the CarPlay area is cut open where it shows.
  const watching = useYouTube((state) => state.open)
  const holdSidebar = useYouTube((state) => state.holdSidebar)
  // YouTube Music's music video in its Now Playing, the same way.
  const youtubeHole = useYouTube((state) => state.hole)
  const musicHole = useYouTubeMusic((state) => state.hole)
  // Safari's page the same way, with what of Safari is drawn over it (apps/safari/hole.ts).
  const pageHole = usePageHole((state) => state.hole)
  const pageCovers = usePageHole((state) => state.covers)
  const hole = youtubeHole ?? musicHole ?? pageHole
  const covers = useMemo(() => (pageHole ? Object.values(pageCovers) : []), [pageHole, pageCovers])
  // Over a video full screen, pointing at the left edge brings the sidebar back (shell/sidebarPeek.ts).
  const carplay = useRef<HTMLDivElement>(null)
  const peek = useSidebarPeek(watching, carplay)
  useEffect(() => {
    // Brought back, it goes again as soon as it's left, without the wait it has as YouTube opens.
    if (peek) useYouTube.setState({ holdSidebar: false })
  }, [peek])

  // Where the host's cursor may be seen (shell/cursorArea.ts): under the black corners, the CarPlay area as it
  // rests (beside the truck's strip; not as it scales out to the truck and back), their radius as laid out (in
  // the CarPlay area's own units, its UI scale); told again whenever they change.
  const corners = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const element = corners.current
    if (!element) return
    const tell = () => {
      const [width, height] = [window.innerWidth, window.innerHeight]
      // The corners' radius (shell.css `--screen-radius`): the sidebar's and the margin, in page pixels.
      const sidebar = element.parentElement?.querySelector('.sidebar')
      const edge = parseFloat(getComputedStyle(element).getPropertyValue('--edge'))
      const sidebarRadius = sidebar ? parseFloat(getComputedStyle(sidebar).borderTopLeftRadius) : 0
      const radius = (sidebarRadius + edge) * profile.scale
      // The screen changes' root snapshot is cut to the same round corners (transitions.css): on <html>, where it
      // hangs, in page pixels.
      document.documentElement.style.setProperty('--carplay-corner', `${radius}px`)
      const area = cursorArea(
        { left: insets.left, top: 0, right: width, bottom: height - insets.bottom, radius },
        { width, height },
        truckScreen,
      )
      if (isInHost()) postToHost({ type: 'cursorArea', area })
    }
    tell()
    const resized = new ResizeObserver(tell)
    resized.observe(element)
    return () => resized.disconnect()
  }, [truckScreen, profile.scale, insets.left, insets.bottom])

  return (
    <main className={`shell appearance-${appearance}`} style={style}>
      {strip && !isInHost() && <GameStripPreview name={strip.name} />}
      <div
        ref={carplay}
        className={`carplay${truckScreen ? ' handed-over' : ''}${watching ? ' watching' : ''}${watching && holdSidebar ? ' hold-sidebar' : ''}${peek ? ' peek-sidebar' : ''}`}
        style={{
          transformOrigin: `${button.x - insets.left}px ${button.y}px`,
          ...(hole && !watching && isInHost() ? holeMask(hole, covers) : {}),
        }}
        aria-hidden={truckScreen}
      >
        <Wallpaper src={wallpaperSrc} />
        <div className={entering ? 'carplay-content entering' : 'carplay-content'}>
          <Sidebar />
          {children}
          <Player />
          <Keyboard />
          <VolumeHud />
          <div ref={corners} className="screen-corners" aria-hidden />
        </div>
      </div>
      {/* The shadows of what's drawn over Safari's page (the keyboard, a menu), falling on the page where it shows
          through: the cab is cut away there, its own shadows with it (apps/safari/hole.ts). */}
      {pageHole && hole === pageHole && !watching && !truckScreen && isInHost() && (
        <div className="page-shadows" style={onlyHole(pageHole)} aria-hidden>
          {Object.entries(pageCovers).map(
            ([key, cover]) =>
              cover.shadow && (
                <div
                  key={key}
                  style={{
                    left: cover.x,
                    top: cover.y,
                    width: cover.width,
                    height: cover.height,
                    borderRadius: `${cover.top ?? cover.radius}px ${cover.top ?? cover.radius}px ${cover.radius}px ${cover.radius}px`,
                    boxShadow: cover.shadow,
                  }}
                />
              ),
          )}
        </div>
      )}
      {/* What's drawn over the video where it shows through the CarPlay area (apps/youtube/VideoHole.tsx). */}
      <div id="video-over" className={truckScreen ? 'video-over handed-over' : 'video-over'} />
      <button
        type="button"
        className={truckScreen ? 'return-button shown' : 'return-button'}
        style={{ left: button.x - RETURN_BUTTON_SIZE / 2, top: button.y - RETURN_BUTTON_SIZE / 2 }}
        aria-label="WestPlay"
        aria-hidden={!truckScreen}
        tabIndex={truckScreen ? 0 : -1}
        onClick={() => useCabin.getState().setTruckScreen(false)}
      >
        <img src="/westplay.png" alt="" />
      </button>
      {overlay}
      {/* Over everything, the cursor included: the image's outermost pixels, which the game repeats into
          the screen's rim (screen.ts `rim`). Nothing moves for it: it covers only those. */}
      {profile.rim && <div className="screen-rim" style={{ color: profile.rim }} aria-hidden />}
    </main>
  )
}

/** Browser previews have no game screen behind the page: mark where the game would draw its strip. */
function GameStripPreview({ name }: { name: string }) {
  return (
    <div className="game-strip-preview" aria-hidden>
      <span>{name} strip (drawn by the game)</span>
    </div>
  )
}

/** A profile's length in page units as a CSS length; none, the stylesheet's own. */
function px(value: number | undefined): string | undefined {
  return value === undefined ? undefined : `${value}px`
}
