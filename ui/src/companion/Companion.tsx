import { type CSSProperties, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { type SignedIn, useAskOnce, useSignedIn } from './accounts'
import { appVersion, call, PREVIEW_TUTORIAL } from './bridge'
import { Onboarding } from './onboarding/Onboarding'
import { languageOf, percent, type Words, wordsFor } from './onboarding/words'
import { About } from './pages/About'
import { useHeldKey } from './pages/heldKey'
import { Lesson, TouchLayer } from './pages/Lesson'
import { Launch, TourCard } from './pages/Tour'
import { Accounts } from './pages/Accounts'
import { Controls } from './pages/Controls'
import { Customize } from './pages/Customize'
import { roundShown } from './mapRound'
import { MapPage } from './pages/MapPage'
import { screenState } from './pages/page'
import { ScreenPage } from './pages/ScreenPage'
import { Setup } from './pages/Setup'
import { Support } from './pages/Support'
import { UninstallSheet } from './pages/UninstallSheet'
import { FinishUpdate, UpdateSheet } from './pages/UpdateSheet'
import { type CompanionWords, companionWords, type SectionId } from './pages/words'
import { RoomText, n, WordsContext } from './room/room'
import './room/room.css'
import { GLOW, type Screen, ScreenContext } from './room/screen'
import { type Banner, GlassCursor, type Pose, TruckScreen } from './room/TruckScreen'
import { useDemo } from './room/useDemo'
import './companion.css'
import { defaultMapAddress, type GameCheck, type SetupState, setupState } from './setup'
import { FILES_BASE, useCustomizations } from './customize'
import { type Offer, useUpdateOffer } from './update'
import { type HostStatus, useHostStatus } from './useHostStatus'
import { interactionKeyLabel, type SettingsState, useSettings } from './useSettings'

/**
 * The companion window: its onboarding on the first run (until it's gone through), then the room it ends in.
 * The truck's screen stays in its dark room, turned aside at the right, the live WestPlay on it; the
 * sections are a list at the left, each with what it is now; the page in between. Each section shows on the
 * screen what it's about (its app, its banner, the cursor using it), as the onboarding's steps did.
 */
export function Companion() {
  const status = useHostStatus()
  const settings = useSettings()
  const [setup, setSetup] = useState<SetupState | null>(null)
  const [setupError, setSetupError] = useState<string | null>(null)
  // After the onboarding, its tutorial (from its start, or skipped to its end), before setup's finished.
  const [tutorial, setTutorial] = useState<'lesson' | 'launch' | null>(PREVIEW_TUTORIAL)
  useEffect(() => {
    setupState()
      .then(setSetup)
      .catch((e: unknown) => setSetupError(String(e)))
  }, [])
  useShowWhenReady(setup !== null || setupError !== null)

  if (!setup) return <div className="cp-blank">{setupError ?? ''}</div>
  if (!setup.finished && !tutorial) {
    // The onboarding set the game up: what's set up is read again (the companion read it before), then its
    // tutorial.
    const onboarded = (start: 'lesson' | 'launch') => {
      setupState()
        .then(setSetup, () => {})
        .finally(() => setTutorial(start))
    }
    return <Onboarding initial={setup} status={status} settings={settings} onFinished={onboarded} />
  }
  return (
    <Room
      setup={setup}
      status={status}
      settings={settings}
      tutorial={tutorial}
      tutorialDone={() => {
        setTutorial(null)
        setupState().then(setSetup, () => setSetup({ ...setup, finished: true }))
      }}
    />
  )
}

const SECTIONS: SectionId[][] = [
  ['screen', 'controls', 'customize'],
  ['accounts', 'map'],
  ['setup', 'about'],
  ['support'],
]
/** The sections in order: the tutorial's tour goes through them. */
const TOUR = SECTIONS.flat()

/** The room as it is: the tutorial's lesson (the screen full size), its tour of the sections, its end
 * (what to do in the game), or the companion. */
type Phase = 'lesson' | 'tour' | 'launch' | 'room'

/** Where the screen stands in each. */
const POSE: Record<Phase, Pose> = { lesson: 'full', tour: 'aside', launch: 'final', room: 'aside' }

/** The light the screen casts while each section is up. */
const SECTION_GLOW: Record<SectionId, string> = {
  screen: GLOW.home,
  controls: GLOW.home,
  customize: GLOW.home,
  accounts: GLOW.youtube,
  map: GLOW.maps,
  setup: GLOW.scan,
  about: GLOW.dashboard,
  support: GLOW.home,
}

/** How long a page's words take to leave before the next one's arrive. */
const LEAVE_MS = 220
/** The screen coming out of the onboarding's dive into its place (companion.css `.cp .ts`). */
const ARRIVE_MS = 1300

function Room({
  setup,
  status,
  settings,
  tutorial = null,
  tutorialDone,
}: {
  setup: SetupState
  status: HostStatus | null
  settings: SettingsState
  /** The tutorial, from where it starts; `null`: the companion. */
  tutorial?: 'lesson' | 'launch' | null
  tutorialDone?: () => void
}) {
  const [code] = useState(() => setup.language ?? languageOf(navigator.languages))
  const words = wordsFor(code)
  const cw = companionWords(code)
  const [section, setSection] = useState<SectionId>('screen')
  const [leaving, setLeaving] = useState(false)
  const [phase, setPhase] = useState<Phase>(tutorial ?? 'room')
  // The screen comes out of the dive the onboarding ended in, to where the phase has it.
  const [arrivedPose, setArrivedPose] = useState(false)
  const pose: Pose = arrivedPose ? POSE[phase] : 'dive'
  const interactionKey = settings.settings?.interaction.key ?? 'leftAlt'
  const held = useHeldKey(interactionKey, phase === 'lesson')
  const demo = useDemo()
  const [powered, setPowered] = useState(false)
  const [banner, setBanner] = useState<Banner | null>(null)
  const [scanning, setScanning] = useState(false)
  const [glow, setGlow] = useState(GLOW.home)
  const [cursor, setCursor] = useState<{ x: number; y: number; pressing: boolean } | null>(null)
  // What a check of the game found, for the pages that show it (Setup, Map): checked once, as they open.
  const [check, setCheck] = useState<GameCheck | null>(null)
  // Who's signed in, known at once (what was last known), and asked as the companion opens if never known.
  const signedIn = useSignedIn(status)
  useAskOnce()
  // The uninstall sheet over the room, and the room fading as WestPlay leaves.
  const [uninstalling, setUninstalling] = useState(false)
  const [leavingPc, setLeavingPc] = useState(false)
  // An update found, offered in its sheet once per version (Later puts it off; About opens it again).
  const update = useUpdateOffer()
  // The player's wallpapers and icon sets, for Customize and the room's screen.
  const { custom, refresh: refreshCustom } = useCustomizations()
  const [updateShown, setUpdateShown] = useState(false)
  const offered = useRef<string | null>(null)
  const root = useRef<HTMLDivElement>(null)
  const arrived = useRef(performance.now() + ARRIVE_MS)

  // Next frame, so the dive is drawn before it moves on; a window not drawing (minimised) gets there anyway.
  useEffect(() => {
    const frame = requestAnimationFrame(() => setArrivedPose(true))
    const timer = window.setTimeout(() => setArrivedPose(true), 150)
    return () => {
      cancelAnimationFrame(frame)
      window.clearTimeout(timer)
    }
  }, [])

  const { send } = demo
  // The room's screen looks as the truck's does: the player's wallpaper and icons, as chosen.
  const look = settings.settings?.look
  useEffect(() => {
    if (!demo.ready || !custom) return
    send({
      type: 'looks',
      custom,
      base: FILES_BASE,
      wallpaper: look?.wallpaper ?? null,
      iconSet: look?.iconSet ?? null,
      iconStyle: look?.iconStyle ?? 'auto',
    })
  }, [demo.ready, custom, look?.wallpaper, look?.iconSet, look?.iconStyle, send])
  const lastGo = useRef<{ route: string; at: number } | null>(null)
  const screen: Screen = {
    send,
    go: (route) => {
      const now = performance.now()
      if (lastGo.current?.route === route && now - lastGo.current.at < 500) return
      lastGo.current = { route, at: now }
      send({ type: 'go', to: route })
    },
    locate: demo.locate,
    ready: demo.ready,
    banner: setBanner,
    scan: setScanning,
    glow: setGlow,
    cursor: setCursor,
    reboot: () => {
      send({ type: 'power', on: false })
      window.setTimeout(() => send({ type: 'power', on: true }), 450)
    },
    powered,
    leaving,
    settling: () => Math.max(0, arrived.current - performance.now()),
  }

  // On as soon as WestPlay in it is ready: it's been on all along (the player dove into it).
  useEffect(() => {
    if (!demo.ready || powered) return
    send({ type: 'power', on: true })
    setPowered(true)
  }, [demo.ready, powered, send])
  useEffect(() => {
    if (demo.ready) send({ type: 'language', code })
  }, [demo.ready, code, send])
  // The player's map, on the screen's Maps.
  const map = defaultMapAddress(status)
  const mapKey = map ? `${map.base}#${map.built}` : ''
  useEffect(() => {
    if (map) send({ type: 'map', base: map.base, built: map.built })
    // Keyed by the map's address and build.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mapKey, demo.ready, send])

  const open = (next: SectionId) => {
    if (next === section || leaving) return
    setLeaving(true)
    window.setTimeout(() => {
      setSection(next)
      setLeaving(false)
    }, LEAVE_MS)
  }

  const tilt = (event: React.PointerEvent) => {
    const box = root.current?.getBoundingClientRect()
    if (!box) return
    root.current?.style.setProperty('--tilt-x', (((event.clientX - box.left) / box.width) * 2 - 1).toFixed(3))
    root.current?.style.setProperty('--tilt-y', (((event.clientY - box.top) / box.height) * 2 - 1).toFixed(3))
  }

  const nextInTour = () => {
    const at = TOUR.indexOf(section)
    if (at === TOUR.length - 1) setPhase('launch')
    else open(TOUR[at + 1])
  }
  const roomShown = phase === 'tour' || phase === 'room'
  useEffect(() => {
    if (!update || phase !== 'room' || offered.current === update.version) return
    offered.current = update.version
    setUpdateShown(true)
  }, [update, phase])

  const page = {
    status,
    settings,
    setup,
    cw,
    check,
    checked: setCheck,
    signedIn,
    uninstall: () => setUninstalling(true),
    update,
    showUpdate: () => setUpdateShown(true),
    custom,
    refreshCustom,
    tutorial: () => setPhase('lesson'),
  }
  return (
    <div
      ref={root}
      className="ob cp"
      lang={words.locale}
      data-power={powered || undefined}
      data-leaving-pc={leavingPc || undefined}
      data-phase={phase}
      style={{ '--section-glow': SECTION_GLOW[section] } as CSSProperties}
      onPointerMove={tilt}
    >
      <TruckScreen frame={demo.frame} ready={demo.ready} pose={pose} glow={glow} banner={banner} scanning={scanning}>
        <GlassCursor
          x={cursor?.x ?? 640}
          y={cursor?.y ?? 420}
          pressing={cursor?.pressing ?? false}
          visible={cursor !== null}
        />
        {phase === 'lesson' && <TouchLayer active={held} press={(at) => send({ type: 'press', ...at })} />}
      </TruckScreen>
      <WordsContext.Provider value={{ words, code }}>
        <RoomText.Provider value={{ locale: words.locale, back: words.back }}>
          <ScreenContext.Provider value={screen}>
            {phase === 'lesson' && (
              <Lesson
                cw={cw}
                interactionKey={interactionKey}
                held={held}
                route={demo.route}
                next={() => {
                  open('screen')
                  setPhase('tour')
                }}
              />
            )}
            {phase === 'launch' && (
              <Launch
                done={() => {
                  setPhase('room')
                  tutorialDone?.()
                }}
              />
            )}
            {roomShown && (
              <SectionList
                section={section}
                open={open}
                cw={cw}
                words={words}
                status={status}
                signedIn={signedIn}
                settings={settings}
                setup={setup}
                update={update}
              />
            )}
            {roomShown && section === 'screen' && <ScreenPage {...page} />}
            {roomShown && section === 'controls' && <Controls {...page} />}
            {roomShown && section === 'customize' && <Customize {...page} />}
            {roomShown && section === 'accounts' && <Accounts {...page} />}
            {roomShown && section === 'map' && <MapPage {...page} />}
            {roomShown && section === 'setup' && <Setup {...page} />}
            {roomShown && section === 'about' && <About {...page} />}
            {roomShown && section === 'support' && <Support {...page} />}
            {phase === 'tour' && (
              <TourCard cw={cw} section={section} last={section === TOUR[TOUR.length - 1]} next={nextInTour} />
            )}
            {uninstalling && (
              <UninstallSheet cw={cw} close={() => setUninstalling(false)} farewell={() => setLeavingPc(true)} />
            )}
            {updateShown && update && !uninstalling && (
              <UpdateSheet cw={cw} offer={update} close={() => setUpdateShown(false)} />
            )}
            {phase === 'room' && <FinishUpdate cw={cw} setup={setup} />}
          </ScreenContext.Provider>
        </RoomText.Provider>
      </WordsContext.Provider>
    </div>
  )
}

/**
 * The sections, at the left: WestPlay's name and where it is now, then each section with what it is now
 * (WestPlay on or off, the key, who's signed in, the map, the game's version, WestPlay's own or an update), in
 * groups. The one that's open is lit by a capsule of glass that glides from section to section.
 */
function SectionList({
  section,
  open,
  cw,
  words,
  status,
  signedIn,
  settings,
  setup,
  update,
}: {
  section: SectionId
  open(section: SectionId): void
  cw: CompanionWords
  words: Words
  status: HostStatus | null
  signedIn: SignedIn
  settings: SettingsState
  setup: SetupState
  update: Offer | null
}) {
  const list = useRef<HTMLElement>(null)
  const [lit, setLit] = useState<{ top: number; height: number } | null>(null)
  const version = useVersion()
  const state = screenState(status)

  useLayoutEffect(() => {
    const row = list.current?.querySelector<HTMLElement>(`[data-section="${section}"]`)
    if (row) setLit({ top: row.offsetTop, height: row.offsetHeight })
  }, [section])

  const value: Record<SectionId, string> = {
    screen:
      state === 'live'
        ? cw.values.on
        : state === 'original'
          ? cw.values.off
          : state === 'notInCab'
            ? cw.values.waiting
            : '',
    controls: interactionKeyLabel(settings.settings?.interaction.key ?? 'leftAlt'),
    customize: '',
    accounts:
      signedIn.youtube === null && signedIn.spotify === null && signedIn.appleMusic === null
        ? ''
        : [signedIn.youtube && 'YouTube', signedIn.spotify && 'Spotify', signedIn.appleMusic && 'Apple Music']
            .filter(Boolean)
            .join(', ') || cw.values.none,
    map: mapValue(status, cw, words),
    setup: setup.game?.version ?? '',
    about: update ? cw.update.badge : version,
    support: '',
  }

  let place = 2
  return (
    <nav ref={list} className="cp-list" aria-label="WestPlay">
      <header className="cp-brand ob-in" style={n(0)}>
        <img src="/westplay.png" alt="" />
        <span>
          <b>WestPlay</b>
          <span className="cp-state" data-state={state}>
            <i />
            {cw.state[state]}
          </span>
        </span>
      </header>
      {lit && <i className="cp-lit" style={{ top: lit.top, height: lit.height }} />}
      {SECTIONS.map((group, g) => (
        <div key={g} className="cp-group">
          {group.map((id) => (
            <button
              key={id}
              type="button"
              className="cp-row ob-in"
              style={n(place++)}
              data-section={id}
              data-here={id === section || undefined}
              onClick={() => open(id)}
            >
              <b>{cw.sections[id]}</b>
              <span>{value[id]}</span>
            </button>
          ))}
        </div>
      ))}
    </nav>
  )
}

function mapValue(status: HostStatus | null, cw: CompanionWords, words: Words): string {
  const map = status?.map
  // The maps being made (one after another): how far, as the Map page's bar.
  const building = [map, ...(status?.profileMaps ?? []).map((each) => each.status)].find(
    (each) => each?.state === 'building',
  )
  const round = building?.state === 'building' ? roundShown(status, building.done) : null
  switch (map?.state) {
    case 'ready':
    case 'building':
      return round !== null
        ? percent(words, round)
        : map.state === 'building'
          ? percent(words, map.done)
          : cw.values.ready
    case 'waitingForGame':
    case 'waitingForMemory':
      return cw.values.waiting
    case 'failed':
    case 'notAllowed':
      return cw.values.failed
    default:
      return ''
  }
}

/** WestPlay's version, once the app has said. */
function useVersion(): string {
  const [version, setVersion] = useState('')
  useEffect(() => {
    appVersion().then(setVersion, () => setVersion(''))
  }, [])
  return version
}

/** The window starts hidden (so it never shows blank while this page loads): it shows once `ready`, after the
 * page has painted. */
function useShowWhenReady(ready: boolean) {
  useEffect(() => {
    if (!ready) return
    // Two frames: the first screen is laid out, then painted.
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        call('companion_ready').catch((error: unknown) => console.error('The window could not be shown', error))
      })
    })
    return () => cancelAnimationFrame(frame)
  }, [ready])
}
