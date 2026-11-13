import { type ReactNode, useContext, useEffect, useRef, useState } from 'react'
import { hvtMapMessage } from '../../cabin/maps/blockedMap'
import {
  checkGame,
  defaultMapAddress,
  chooseGameFolder,
  type GameCheck,
  type GameInfo,
  type InstallReport,
  installIntoGame,
  GAME_KINDS,
  GAME_NAMES,
  type SetupState,
} from '../setup'
import {
  type HostStatus,
  appleMusicCheck,
  appleMusicSignIn,
  appleMusicSignOut,
  spotifyCheck,
  spotifySignIn,
  spotifySignOut,
  youtubeCheck,
  youtubeSignIn,
  youtubeSignOut,
} from '../useHostStatus'
import { DEFAULT_KEY, type SettingsState } from '../useSettings'
import { memoryNote } from '../pages/page'
import { companionWords } from '../pages/words'
import '../room/room.css'
import { type Banner, GlassCursor, type Pose, TruckScreen } from '../room/TruckScreen'
import {
  Button,
  CheckBox,
  Checking,
  Cross,
  Group,
  KeyChooser,
  MapProgress,
  Note,
  Panel,
  ProfileRow,
  Row,
  Status,
  Switch,
  Tick,
} from '../room/parts'
import {
  banner,
  explain,
  gameSummary,
  n,
  RoomText,
  useChecking,
  useGameRunning,
  useGlidingProgress,
  useSignOut,
  useWords,
  WordsContext,
} from '../room/room'
import {
  GLOW,
  ON_SCREEN,
  openAndClose,
  type Screen,
  ScreenContext,
  useScreen,
  useScreenAt,
  useTour,
} from '../room/screen'
import { useDemo } from '../room/useDemo'
import { chosenLanguage, count, list, percent, wordsFor } from './words'

const STEPS = [
  'welcome',
  'game',
  'check',
  'speed',
  'install',
  'map',
  'controls',
  'accounts',
  'spotify',
  'appleMusic',
  'tutorial',
] as const
type StepId = (typeof STEPS)[number]

const POSES: Record<StepId, Pose> = {
  welcome: 'hero',
  game: 'side',
  check: 'side',
  speed: 'side',
  install: 'side',
  map: 'side',
  controls: 'side',
  accounts: 'side',
  spotify: 'side',
  appleMusic: 'side',
  tutorial: 'final',
}

/** How long the words take to leave before the next step's arrive. */
const LEAVE_MS = 240
/** The WestPlay intro on the screen, from power on to Home (cabin/state/power.ts CONNECTING_MS, and a
 * beat). */
const BOOT_MS = 2900
/** How long the screen takes to move to another place (onboarding.css `.ts` transform). */
const POSE_MS = 1300
/** What's on the screen changes this long before its move ends (the move's last stretch hardly moves). */
const POSE_OVERLAP_MS = 250

/** The dive into the screen, before the companion. */
const DIVE_MS = 1100

/**
 * The companion's first run. The truck's screen stands in a dark room, the live WestPlay on it (the cabin
 * page in its demo mode): it boots, steps aside while the player sets WestPlay up, and tells them how it's
 * going through banners as CarPlay would; it opens what each step is about (their map as it's made, an app
 * tapped with the key they choose), and comes back to centre stage on the Dashboard, the player diving into
 * it as they finish. The game stays closed until the end: WestPlay installs into it and builds its map
 * while it's closed.
 */
export function Onboarding({
  initial,
  status,
  settings,
  onFinished,
  mapAddress = defaultMapAddress,
  over,
}: {
  initial: SetupState
  status: HostStatus | null
  settings: SettingsState
  /** The onboarding's done: its tutorial next, from its start or skipped to its end. */
  onFinished(start: 'lesson' | 'launch'): void
  mapAddress?: (status: HostStatus | null) => { base: string; built: number | null } | null
  /** A sheet over everything (`GameStoppedSheet`). */
  over?: ReactNode
}) {
  const [step, setStep] = useState<StepId>('welcome')
  const [leaving, setLeaving] = useState(false)
  const [finishing, setFinishing] = useState(false)
  // The games to set WestPlay up in (one install of each), each one's check and what installing did, kept while
  // the player moves between steps (until they choose another install of it).
  const [games, setGames] = useState<GameInfo[]>(() => chosenAtFirst(initial))
  const [checks, setChecks] = useState<Record<string, GameCheck>>({})
  const [installed, setInstalled] = useState<Record<string, InstallReport>>({})
  const demo = useDemo()
  const [powered, setPowered] = useState(false)
  const [banner, setBanner] = useState<Banner | null>(null)
  const [scanning, setScanning] = useState(false)
  const [glow, setGlow] = useState(GLOW.boot)
  const [cursor, setCursor] = useState<{ x: number; y: number; pressing: boolean } | null>(null)
  const root = useRef<HTMLDivElement>(null)
  const index = STEPS.indexOf(step)
  // The language chosen in Setup (WestPlay installed again), or the game's, as WestPlay speaks in the cab (and
  // the map's place names are in); Windows' for a game never started.
  const code = chosenLanguage(settings.settings?.language, initial.language)
  const words = wordsFor(code)

  // When the screen's move to its next place ends. Its content changes after that, not during it: the
  // page draws both, and an app or the Dashboard starting up (the map) holds up the move, which then jumps.
  const moveEnds = useRef(0)
  const go = (to: number) => {
    if (leaving || to < 0 || to >= STEPS.length) return
    if (POSES[STEPS[to]] !== POSES[step]) moveEnds.current = performance.now() + LEAVE_MS + POSE_MS
    setLeaving(true)
    window.setTimeout(() => {
      setStep(STEPS[to])
      setLeaving(false)
    }, LEAVE_MS)
  }
  const next = () => go(index + 1)
  const back = index > 1 ? () => go(index - 1) : undefined

  const { send } = demo
  // The last place the screen was sent, and when: the same again right after is dropped. React runs a
  // step's effects twice while developing, and the screen takes a second press during an animation as
  // a player's, finishing it at once: an app would appear without opening.
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
    settling: () => Math.max(0, moveEnds.current - POSE_OVERLAP_MS - performance.now()),
  }

  // The screen comes on once WestPlay in it is ready: the intro, then Home.
  useEffect(() => {
    if (!demo.ready || powered) return
    const timer = window.setTimeout(() => {
      send({ type: 'power', on: true })
      setPowered(true)
    }, 700)
    return () => window.clearTimeout(timer)
  }, [demo.ready, powered, send])

  // WestPlay on the screen speaks the same language.
  useEffect(() => {
    if (demo.ready) send({ type: 'language', code })
  }, [demo.ready, code, send])

  // The map, once there is one, is shown on the screen.
  const map = mapAddress(status)
  const mapKey = map ? `${map.base}#${map.built}` : ''
  useEffect(() => {
    if (map) send({ type: 'map', base: map.base, built: map.built })
    // Keyed by the map's address and build.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mapKey, demo.ready, send])

  // The mouse turns the screen a little.
  const tilt = (event: React.PointerEvent) => {
    const box = root.current?.getBoundingClientRect()
    if (!box) return
    root.current?.style.setProperty('--tilt-x', (((event.clientX - box.left) / box.width) * 2 - 1).toFixed(3))
    root.current?.style.setProperty('--tilt-y', (((event.clientY - box.top) / box.height) * 2 - 1).toFixed(3))
  }

  const finish = (start: 'lesson' | 'launch') => {
    setFinishing(true)
    window.setTimeout(() => onFinished(start), DIVE_MS)
  }

  return (
    <div
      ref={root}
      className="ob"
      lang={words.locale}
      data-power={powered || undefined}
      data-finishing={finishing || undefined}
      onPointerMove={tilt}
    >
      <TruckScreen
        frame={demo.frame}
        ready={demo.ready}
        pose={finishing ? 'dive' : POSES[step]}
        glow={glow}
        banner={banner}
        scanning={scanning}
      >
        <GlassCursor
          x={cursor?.x ?? 640}
          y={cursor?.y ?? 420}
          pressing={cursor?.pressing ?? false}
          visible={cursor !== null}
        />
      </TruckScreen>
      <WordsContext.Provider value={{ words, code }}>
        <RoomText.Provider value={{ locale: words.locale, back: words.back }}>
          <ScreenContext.Provider value={screen}>
            {step === 'welcome' && <Welcome next={next} />}
            {step === 'game' && (
              <ChooseGame
                found={initial.found}
                games={games}
                choose={(chosen) => {
                  // In place of the install of its game chosen before (the other game's kept).
                  setGames(
                    GAME_KINDS.flatMap((kind) =>
                      kind === chosen.kind ? chosen : (games.find((game) => game.kind === kind) ?? []),
                    ),
                  )
                }}
                next={next}
                back={back}
              />
            )}
            {step === 'check' && games.length > 0 && (
              <CheckGame
                games={games}
                checks={checks}
                checked={(dir, check) => setChecks((known) => ({ ...known, [dir]: check }))}
                next={next}
                back={back}
              />
            )}
            {step === 'speed' && <Speed settings={settings} next={next} back={back} />}
            {step === 'install' && games.length > 0 && (
              <Install
                games={games}
                reports={installed}
                reported={(dir, report) => setInstalled((known) => ({ ...known, [dir]: report }))}
                next={next}
                back={back}
              />
            )}
            {step === 'map' && <MapStep status={status} next={next} back={back} />}
            {step === 'controls' && <Controls settings={settings} next={next} back={back} />}
            {step === 'accounts' && <Accounts status={status} next={next} back={back} />}
            {step === 'spotify' && <ServiceSignIn service="spotify" status={status} next={next} back={back} />}
            {step === 'appleMusic' && <ServiceSignIn service="appleMusic" status={status} next={next} back={back} />}
            {step === 'tutorial' && <TutorialIntro code={code} finish={finish} back={back} />}
            {over}
          </ScreenContext.Provider>
        </RoomText.Provider>
      </WordsContext.Provider>
      <div className="ob-dots" aria-hidden>
        {STEPS.map((id, at) => (
          <i key={id} data-here={at === index || undefined} data-passed={at < index || undefined} />
        ))}
      </div>
    </div>
  )
}

function Welcome({ next }: { next(): void }) {
  const screen = useScreen()
  // The words come once WestPlay has booted on the screen.
  const [booted, setBooted] = useState(false)
  useEffect(() => {
    if (!screen.powered) return
    const timer = window.setTimeout(() => setBooted(true), BOOT_MS)
    return () => window.clearTimeout(timer)
  }, [screen.powered])
  const words = useWords()
  useScreenAt('home', GLOW.home, null)
  useTour(
    booted
      ? [
          { tap: ON_SCREEN.page(2), then: 3400 },
          { tap: ON_SCREEN.page(1), then: 4200 },
        ]
      : null,
    2200,
  )
  if (!booted) return null
  return (
    <Panel
      place="below"
      title={words.welcome.title}
      sub={words.welcome.sub}
      action={<Button onClick={next}>{words.welcome.start}</Button>}
    />
  )
}

/** The games to set WestPlay up in: one install of each game found (owner 2026-10-28: both in one go), those
 * set up before kept. */
function chosenAtFirst(initial: SetupState): GameInfo[] {
  return GAME_KINDS.flatMap((kind) => {
    const setUp = initial.games.find((game) => game.kind === kind)
    const found = initial.found.find((game) => game.kind === kind)
    return setUp ?? found ?? []
  })
}

/** The games' names, as the screen's banner shows them. */
function namesOf(games: GameInfo[]): string {
  return games.map((game) => GAME_NAMES[game.kind]).join(' · ')
}

function ChooseGame({
  found,
  games,
  choose,
  next,
  back,
}: {
  found: GameInfo[]
  /** The installs chosen, one of each game at most. */
  games: GameInfo[]
  choose(game: GameInfo): void
  next(): void
  back?: () => void
}) {
  // Every install known: those found, and any chosen by hand.
  const [known, setKnown] = useState(() => [
    ...found,
    ...games.filter((game) => !found.some((each) => each.dir === game.dir)),
  ])
  const [error, setError] = useState<string | null>(null)
  const running = useGameRunning()
  const words = useWords()
  const w = words.game
  useScreenAt(
    'dashboard',
    GLOW.dashboard,
    games.length
      ? games.length > 1
        ? { title: namesOf(games), detail: games.map((game) => game.version ?? '').join(' · ') }
        : { title: namesOf(games), detail: gameSummary(words, games[0]) }
      : { title: w.titleMissing, detail: w.bannerMissing },
  )

  const pick = async () => {
    try {
      const chosen = await chooseGameFolder(w.picker)
      if (!chosen) return
      setError(null)
      if (!known.some((each) => each.dir === chosen.dir)) setKnown([...known, chosen])
      choose(chosen)
    } catch (e) {
      setError(explain(words, e))
    }
  }

  // Each game's installs, in the companion's order of the games.
  const byGame = GAME_KINDS.map((kind) => known.filter((game) => game.kind === kind)).filter((each) => each.length)
  const sub = !known.length
    ? w.missing
    : byGame.some((installs) => installs.length > 1)
      ? w.foundMany
      : byGame.length > 1
        ? w.foundBoth
        : w.found
  return (
    <Panel
      place="side"
      title={!known.length ? w.titleMissing : byGame.length > 1 ? w.titleBoth : w.title}
      sub={sub}
      back={back}
      action={
        <Button onClick={next} disabled={!games.length || running !== false}>
          {words.continue}
        </Button>
      }
    >
      {byGame.map((installs, at) => (
        <Group key={installs[0].kind} title={GAME_NAMES[installs[0].kind]} place={3 + at}>
          {installs.map((install) => (
            <Row
              key={install.dir}
              title={install.dir.replace(/[\\/]steamapps[\\/]common[\\/][^\\/]+[\\/]?$/i, '')}
              detail={gameSummary(words, install)}
              accessory={games.some((game) => game.dir === install.dir) ? <Tick key={install.dir} /> : undefined}
              onClick={() => choose(install)}
            />
          ))}
        </Group>
      ))}
      <Group place={3 + byGame.length}>
        <Row title={known.length ? w.chooseAnother : w.chooseFolder} link onClick={pick} />
      </Group>
      {error && <Note tone="bad">{error}</Note>}
      {running && (
        <Note>
          <b>{w.closeToGoOn.title}</b> {w.closeToGoOn.text}
        </Note>
      )}
    </Panel>
  )
}

function CheckGame({
  games,
  checks,
  checked,
  next,
  back,
}: {
  games: GameInfo[]
  /** Each game's check, by its folder, once it's done. */
  checks: Record<string, GameCheck>
  checked(dir: string, check: GameCheck): void
  next(): void
  back?: () => void
}) {
  const screen = useScreen()
  const words = useWords()
  const w = words.check
  const [error, setError] = useState<string | null>(null)
  const done = games.every((game) => checks[game.dir])
  const key = games.map((game) => game.dir).join('|')
  useEffect(() => {
    if (done) return
    let active = true
    // Long enough to see the scan pass at least once.
    const shown = new Promise((resolve) => window.setTimeout(resolve, 2200))
    const each = games
      .filter((game) => !checks[game.dir])
      .map((game) => checkGame(game.dir).then((result) => [game.dir, result] as const))
    Promise.all([Promise.all(each), shown])
      .then(([results]) => active && results.forEach(([dir, result]) => checked(dir, result)))
      .catch((e: unknown) => active && setError(explain(words, e)))
    return () => {
      active = false
    }
    // Checked again only as the games chosen change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, done, words])
  useEffect(() => {
    screen.scan(!done && !error)
    return () => screen.scan(false)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [done, error])
  const all = games.map((game) => checks[game.dir]).filter((check): check is GameCheck => !!check)
  const problem = all.some((check) => check.modProblem)
  const profiles = all.reduce((sum, check) => sum + check.profiles.length, 0)
  const dlcs = all.reduce((sum, check) => sum + check.dlcs, 0)
  useScreenAt(
    'home',
    done ? GLOW.home : GLOW.scan,
    done
      ? {
          title: problem ? w.notYet : w.checked,
          detail: `${count(words, w.profileCount, profiles)} · ${count(words, words.summary.dlcs, dlcs)}`,
        }
      : { title: w.checking, busy: true },
  )

  if (!done) {
    return (
      <Panel
        place="side"
        title={w.looking}
        sub={w.lookingSub}
        back={back}
        action={
          <Button onClick={next} busy={!error}>
            {words.continue}
          </Button>
        }
      >
        {error && <Note tone="bad">{error}</Note>}
      </Panel>
    )
  }

  // Each game's part: its version and DLCs, its trucks and its profiles (its name above it with two games).
  return (
    <Panel
      place="side"
      title={problem ? w.titleNotYet : w.title}
      back={back}
      action={
        <Button onClick={next} disabled={problem}>
          {words.continue}
        </Button>
      }
    >
      {all.map((check, at) => (
        <GameChecked key={check.game.dir} check={check} titled={all.length > 1} place={2 + at * 4} />
      ))}
      <p className="ob-sub ob-in" style={{ ...n(2 + all.length * 4), fontSize: 12.5, marginTop: 0 }}>
        {w.mapNote}
      </p>
    </Panel>
  )
}

/** One game's check, its groups placed from `place` (four of them). */
function GameChecked({ check, titled, place }: { check: GameCheck; titled: boolean; place: number }) {
  const words = useWords()
  const w = words.check
  return (
    <>
      {check.modProblem && (
        <Note tone="bad">
          <b>{w.cantInstall}</b> {check.modProblem}
        </Note>
      )}
      {check.otherApps.includes('ymnOne') && (
        <Note tone="bad">
          <b>{w.otherCarPlay.title}</b> {w.otherCarPlay.text}
        </Note>
      )}
      <Group title={titled ? GAME_NAMES[check.game.kind] : undefined} place={place}>
        <Row
          title={w.version}
          value={check.game.version ?? w.unknown}
          accessory={check.game.tested ? <Tick done /> : <Status tone="idle" />}
        />
        <Row title={w.dlcs} value={new Intl.NumberFormat(words.locale).format(check.dlcs)} />
        <Row
          title={w.mapDlcs}
          detail={check.mapDlcs.length ? list(words, check.mapDlcs) : w.noMapDlcs}
          value={new Intl.NumberFormat(words.locale).format(check.mapDlcs.length)}
        />
      </Group>
      {!check.game.tested && !check.modProblem && <Note>{w.untested}</Note>}
      <Group title={w.trucks} place={place + 1}>
        {check.trucks.map((truck) => (
          <Row
            key={truck.id}
            title={
              truck.id === 'scania-7'
                ? w.scania7
                : truck.id === 'scania-smart-dash'
                  ? w.scaniaSmartDash
                  : truck.id === 'any-truck-tablet'
                    ? w.anyTruckTablet
                    : truck.name
            }
            detail={
              truck.fit === 'ready'
                ? undefined
                : truck.fit === 'notYet'
                  ? w.truckNotYet
                  : truck.fromMod
                    ? w.notInMods
                    : w.notInGame
            }
            accessory={truck.fit === 'ready' ? <Tick done /> : <Cross />}
          />
        ))}
      </Group>
      <p className="ob-sub ob-in" style={{ ...n(place + 2), fontSize: 12.5, marginTop: 0 }}>
        {w.otherTrucks}
      </p>
      <Group title={w.profiles} place={place + 3}>
        {check.profiles.length ? (
          check.profiles.map((profile) => <ProfileRow key={profile.dir} profile={profile} />)
        ) : (
          <Row title={w.noProfiles} detail={w.noProfilesDetail} />
        )}
      </Group>
    </>
  )
}

function Install({
  games,
  reports,
  reported,
  next,
  back,
}: {
  games: GameInfo[]
  /** What installing did in each game (by its folder), if it was done (on this visit or an earlier one). */
  reports: Record<string, InstallReport>
  reported(dir: string, report: InstallReport): void
  next(): void
  back?: () => void
}) {
  const screen = useScreen()
  const words = useWords()
  const w = words.install
  const [installing, setInstalling] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const done = games.every((game) => reports[game.dir]?.done)
  // Back on a finished install, it's simply done: no reboot again.
  const [celebrated, setCelebrated] = useState(done)
  const running = useGameRunning()

  // Into each game not done yet, one after the other; one that can't go on (Windows' permission, the game in use)
  // stops there, its note saying why.
  const install = async (administrator: boolean) => {
    setInstalling(true)
    setError(null)
    try {
      let all = true
      for (const game of games) {
        if (reports[game.dir]?.done) continue
        const result = await installIntoGame(game.dir, administrator)
        reported(game.dir, result)
        if (!result.done) {
          all = false
          break
        }
      }
      if (all) {
        // WestPlay in the games: the screen boots it again.
        screen.reboot()
        window.setTimeout(() => setCelebrated(true), BOOT_MS + 450)
      }
    } catch (e) {
      setError(explain(words, e))
    } finally {
      setInstalling(false)
    }
  }

  // The report that says what to do now: the first game's not done.
  const waiting = games.map((game) => reports[game.dir]).find((report) => report && !report.done) ?? null
  const plugin = waiting?.plugin
  useScreenAt(
    'home',
    done ? GLOW.boot : GLOW.home,
    installing
      ? banner({ ...w.banners.installing, text: w.plugin }, true)
      : done
        ? celebrated
          ? banner(w.banners.done)
          : null
        : plugin?.state === 'needsAdministrator' || plugin?.state === 'declined'
          ? banner(w.banners.permission)
          : plugin?.state === 'inUse'
            ? banner(w.banners.inUse)
            : waiting
              ? banner(w.banners.failed)
              : banner({ ...w.banners.ready, text: w.pluginDetail }),
  )

  // Waiting on the player (Windows' permission, the game closed) isn't a failure.
  const toneOf = (report: InstallReport | undefined) =>
    installing && !report?.done
      ? 'busy'
      : report?.plugin.state === 'installed'
        ? 'good'
        : report?.plugin.state === 'failed'
          ? 'bad'
          : 'idle'
  const asAdministrator = plugin?.state === 'needsAdministrator' || plugin?.state === 'declined'

  return (
    <Panel
      place="side"
      title={done ? w.titleDone : w.title}
      sub={w.sub}
      // While WestPlay boots again on the screen, the player waits for it: the next step opens an app there.
      back={installing || (done && !celebrated) ? undefined : back}
      action={
        done ? (
          <Button onClick={next} busy={!celebrated}>
            {words.continue}
          </Button>
        ) : (
          <Button onClick={() => install(asAdministrator)} busy={installing} disabled={running !== false}>
            {asAdministrator ? w.asAdministrator : waiting || error ? w.tryAgain : w.install}
          </Button>
        )
      }
    >
      <Group place={4}>
        {games.map((game) => {
          const tone = toneOf(reports[game.dir])
          return (
            <Row
              key={game.dir}
              title={games.length > 1 ? GAME_NAMES[game.kind] : w.plugin}
              detail={w.pluginDetail}
              accessory={tone === 'good' ? <Tick done /> : <Status tone={tone} />}
            />
          )
        })}
      </Group>
      {running && !done && (
        <Note>
          <b>{w.closeToInstall.title}</b> {w.closeToInstall.text}
        </Note>
      )}
      {plugin?.state === 'inUse' && (
        <Note>
          <b>{w.inUse.title}</b> {w.inUse.text}
        </Note>
      )}
      {plugin?.state === 'needsAdministrator' && (
        <Note tone="calm">
          <b>{w.needsAdministrator.title}</b> {w.needsAdministrator.text}
        </Note>
      )}
      {plugin?.state === 'declined' && (
        <Note>
          <b>{w.declined.title}</b> {w.declined.text}
        </Note>
      )}
      {plugin?.state === 'failed' && (
        <Note tone="bad">
          <b>{w.pluginFailed}</b> {explain(words, plugin.error)}
        </Note>
      )}
      {waiting?.modError && (
        <Note tone="bad">
          <b>{w.modFailed}</b> {explain(words, waiting.modError)}
        </Note>
      )}
      {error && <Note tone="bad">{error}</Note>}
    </Panel>
  )
}

function MapStep({ status, next, back }: { status: HostStatus | null; next(): void; back?: () => void }) {
  const map = status?.map ?? { state: 'unknown' as const }
  const building = map.state === 'building' ? map : null
  const shown = useGlidingProgress(building?.done ?? null, building?.stage ?? null)
  // Seen building on this visit: once it's ready, its stages stay up, all ticked.
  const [watched, setWatched] = useState(false)
  if (building && !watched) setWatched(true)
  const settled = map.state === 'ready' || map.state === 'notAllowed' || map.state === 'failed'
  const { words, code } = useContext(WordsContext)
  const w = words.map
  // The HVT message, its two sentences as the banner's title and line.
  const [hvtTitle, hvtText] = [
    ...new Intl.Segmenter(words.locale, { granularity: 'sentence' }).segment(hvtMapMessage(code)),
  ].map((part) => part.segment.trim())
  useScreenAt(
    'maps',
    GLOW.maps,
    map.state === 'building'
      ? {
          title: w.banners.building,
          detail: `${w.stages[map.stage].now} · ${percent(words, shown)}`,
          progress: shown,
        }
      : map.state === 'ready'
        ? banner(w.banners.ready)
        : map.state === 'waitingForGame'
          ? banner(w.banners.waiting)
          : map.state === 'waitingForMemory'
            ? banner(companionWords(code).map.memory.banner)
            : map.state === 'notAllowed'
              ? { title: hvtTitle, detail: hvtText }
              : map.state === 'failed'
                ? banner(w.banners.failed)
                : { title: w.banners.getting, busy: true },
  )

  useTour(
    map.state === 'ready'
      ? [
          { tap: ON_SCREEN.zoomOut, then: 700 },
          { tap: ON_SCREEN.zoomOut, then: 700 },
          { tap: ON_SCREEN.zoomOut, then: 3200 },
          { tap: ON_SCREEN.zoomIn, then: 700 },
          { tap: ON_SCREEN.zoomIn, then: 700 },
          { tap: ON_SCREEN.zoomIn, then: 3600 },
        ]
      : null,
    1800,
  )

  return (
    <Panel
      place="side"
      title={map.state === 'ready' ? w.titleReady : w.title}
      sub={w.sub}
      back={back}
      action={
        <Button onClick={next} disabled={!settled}>
          {words.continue}
        </Button>
      }
    >
      {(building || (watched && map.state === 'ready')) && (
        <MapProgress shown={shown} stage={building?.stage ?? null} finished={map.state === 'ready'} />
      )}
      {map.state === 'failed' && <Note tone="bad">{map.error}</Note>}
      {map.state === 'waitingForGame' && (
        <Note>
          <b>{w.waiting.title}</b> {w.waiting.text}
        </Note>
      )}
      {map.state === 'waitingForMemory' && (
        <Note>
          <b>{companionWords(code).map.memory.note.title}</b>{' '}
          {memoryNote(companionWords(code), words.locale, map.needed, map.free)}
        </Note>
      )}
    </Panel>
  )
}

/**
 * Before WestPlay is installed (its maps are drawn as soon as it is: `MapWatch`, the game set up): whether they're
 * drawn faster, on the whole processor and up to 80% of the memory, off unless the player turns it on; and a box
 * to tick saying they've read the page, Continue waiting for it (owner 2026-11-06: "people don't read"). The
 * choice is the Map page's Prepare Maps Faster, changed there later.
 */
function Speed({ settings, next, back }: { settings: SettingsState; next(): void; back?: () => void }) {
  const current = settings.settings
  const { words, code } = useContext(WordsContext)
  const w = words.speed
  const cw = companionWords(code).map
  const [read, setRead] = useState(false)
  useScreenAt('maps', GLOW.maps, null)
  return (
    <Panel
      place="side"
      title={w.title}
      sub={w.sub}
      back={back}
      action={
        <Button onClick={next} disabled={!read || !current}>
          {words.continue}
        </Button>
      }
    >
      <Group place={4}>
        <Row
          title={cw.fast}
          detail={cw.fastDetail}
          accessory={
            <Switch
              on={current?.map.fast ?? false}
              disabled={!current}
              label={cw.fast}
              change={(fast) => current && settings.update({ ...current, map: { ...current.map, fast } })}
            />
          }
        />
      </Group>
      <Group place={5}>
        <Row title={w.confirm} wrap accessory={<CheckBox on={read} label={w.confirm} change={setRead} />} />
      </Group>
      {settings.error && <Note tone="bad">{settings.error}</Note>}
    </Panel>
  )
}

function Controls({ settings, next, back }: { settings: SettingsState; next(): void; back?: () => void }) {
  const current = settings.settings
  const words = useWords()
  useScreenAt('home', GLOW.home, null)
  // Holding the key: the player taps one app, then another.
  useTour([
    ...openAndClose('music', GLOW.music),
    ...openAndClose('weather', GLOW.weather),
    ...openAndClose('maps', GLOW.maps),
  ])
  return (
    <Panel
      place="side"
      title={words.controls.title}
      sub={words.controls.sub}
      back={back}
      action={<Button onClick={next}>{words.continue}</Button>}
    >
      <KeyChooser
        value={current?.interaction.key ?? DEFAULT_KEY}
        disabled={!current}
        place={4}
        choose={(key) => current && settings.update({ ...current, interaction: { ...current.interaction, key } })}
      />
      {settings.error && <Note tone="bad">{settings.error}</Note>}
    </Panel>
  )
}

function Accounts({ status, next, back }: { status: HostStatus | null; next(): void; back?: () => void }) {
  const signedIn = status?.youtubeSignedIn ?? false
  const signingIn = status?.youtubeSigningIn ?? false
  // Signed in already? Found out first (YouTube run, hidden, to say so).
  const checking = useChecking(status?.youtubeSignedIn != null, youtubeCheck)
  const signOut = useSignOut(signedIn, youtubeSignOut)
  const words = useWords()
  const w = words.accounts
  useScreenAt('youtube', GLOW.youtube, signingIn ? banner(w.signingIn, true) : null)
  return (
    <Panel
      place="side"
      title={w.title}
      sub={w.sub}
      back={back}
      action={
        <Button onClick={next} disabled={checking}>
          {signedIn ? words.continue : w.notNow}
        </Button>
      }
    >
      <Group place={4}>
        <Row
          title="YouTube"
          detail={checking ? undefined : signingIn ? w.finishInWindow : signedIn ? w.signedIn : w.notSignedIn}
          accessory={
            checking ? (
              <Checking />
            ) : signedIn ? (
              <Button glass small busy={signOut.signingOut} onClick={signOut.start}>
                {w.signOut}
              </Button>
            ) : (
              <Button glass small busy={signingIn} onClick={() => youtubeSignIn()}>
                {w.signIn}
              </Button>
            )
          }
        />
      </Group>
    </Panel>
  )
}

/** Spotify's and Apple Music's sign-ins, as YouTube's: the service's own page in a window on the desktop, its
 * app on the screen meanwhile. Their buttons and states are YouTube's step's words; their own say what each
 * is for. */
const SERVICE_STEPS = {
  spotify: {
    name: 'Spotify',
    app: 'spotify',
    glow: GLOW.spotify,
    signedIn: (status: HostStatus | null) => status?.spotifySignedIn,
    signingIn: (status: HostStatus | null) => status?.spotifySigningIn ?? false,
    check: spotifyCheck,
    signIn: spotifySignIn,
    signOut: spotifySignOut,
  },
  appleMusic: {
    name: 'Apple Music',
    app: 'music',
    glow: GLOW.music,
    signedIn: (status: HostStatus | null) => status?.appleMusicSignedIn,
    signingIn: (status: HostStatus | null) => status?.appleMusicSigningIn ?? false,
    check: appleMusicCheck,
    signIn: appleMusicSignIn,
    signOut: appleMusicSignOut,
  },
} as const

function ServiceSignIn({
  service,
  status,
  next,
  back,
}: {
  service: keyof typeof SERVICE_STEPS
  status: HostStatus | null
  next(): void
  back?: () => void
}) {
  const it = SERVICE_STEPS[service]
  const signedIn = it.signedIn(status) ?? false
  const signingIn = it.signingIn(status)
  // Signed in already? Found out first (the service's web player run, hidden, to say so).
  const checking = useChecking(it.signedIn(status) != null, it.check)
  const signOut = useSignOut(signedIn, it.signOut)
  const words = useWords()
  const w = words.accounts
  const s = words[service]
  useScreenAt(it.app, it.glow, signingIn ? banner(s.signingIn, true) : null)
  return (
    <Panel
      place="side"
      title={s.title}
      sub={s.sub}
      back={back}
      action={
        <Button onClick={next} disabled={checking}>
          {signedIn ? words.continue : w.notNow}
        </Button>
      }
    >
      <Group place={4}>
        <Row
          title={it.name}
          detail={checking ? undefined : signingIn ? s.finishInWindow : signedIn ? w.signedIn : w.notSignedIn}
          accessory={
            checking ? (
              <Checking />
            ) : signedIn ? (
              <Button glass small busy={signOut.signingOut} onClick={signOut.start}>
                {w.signOut}
              </Button>
            ) : (
              <Button glass small busy={signingIn} onClick={() => it.signIn()}>
                {w.signIn}
              </Button>
            )
          }
        />
      </Group>
    </Panel>
  )
}

/** The onboarding's last step: the tutorial's start (the player dives into the screen and learns it), or
 * skipped to its end (what to do in the game). */
function TutorialIntro({
  code,
  finish,
  back,
}: {
  code: string
  finish(start: 'lesson' | 'launch'): void
  back?: () => void
}) {
  const w = companionWords(code).tutorial.intro
  useScreenAt('dashboard', GLOW.dashboard, null)
  useTour(
    [
      { tap: ON_SCREEN.homeButton, glow: GLOW.home, then: 3400 },
      { tap: ON_SCREEN.homeButton, glow: GLOW.dashboard, then: 4600 },
    ],
    2600,
  )
  return (
    <Panel
      place="below"
      title={w.title}
      sub={w.sub}
      back={back}
      action={
        <>
          <Button glass onClick={() => finish('launch')}>
            {w.skip}
          </Button>
          <Button onClick={() => finish('lesson')}>{w.start}</Button>
        </>
      }
    />
  )
}
