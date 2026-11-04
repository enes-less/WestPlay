import { create } from 'zustand'
import { useLive } from '../state/store'
import {
  type Area,
  areaQuery,
  around,
  type FeedPlayer,
  feedClock,
  type OtherPlayer,
  otherPlayers,
  ownId,
  type PathPoint,
  pathDistance,
  readFeed,
  type Sample,
  selfOnPath,
  withRead,
} from './truckersmpFeed'

export type { Area, OtherPlayer } from './truckersmpFeed'

/**
 * TruckersMP's other players, for the map and the traffic on a route offered (PLAN.md: Navigation plan, B;
 * owner 2026-10-21). Read from the feed TruckersMP's own live map (map.truckersmp.com) reads: the tracked
 * players in a part of a server's map, with their position in the game's coordinates (the feed's `X` is the
 * game's x, its `Y` the game's z), heading (the game's heading in radians) and their number on the server.
 * Names and the rest are dropped as they're read: only where each truck is, where it's heading and how it
 * moves are kept, and its number, to measure that and to leave the player's own truck out.
 *
 * Only what WestPlay shows is read (owner 2026-10-21): every second, the part of the map a screen shows
 * (the map says which: [`setMapView`]) and the player's own surroundings; while a route is offered, its
 * corridor (the card says which: [`setRouteCorridor`]) every few seconds, for its traffic. The player's
 * server is the one TruckersMP's SDK names; without it, the one of the running game's servers with the
 * player's own truck on it: a truck where theirs was when the feed placed it, facing their way (its path,
 * [`selfOnPath`]), on that server alone (owner 2026-11-03: any truck near theirs had made every player
 * Simulation 1's, the first server listed, whose 1,100 players are near anyone). It's checked on every read.
 *
 * The page reads it only while the host says the game runs under TruckersMP.
 */

const FEED = 'https://tracker.ets2map.com/v3/area'
const SERVERS = 'https://truckersmp.krashnz.com/servers'
/** How often the map's part is read: every 3 s (owner 2026-10-21: the trucks around the player come from
 * TruckersMP's SDK, live; these are only the further ones, which the map moves smoothly between reads). */
const READ_EVERY_MS = 3000
/** Every how many reads the corridor of a route offered is read too (every 6 s: its traffic changes slowly). */
const CORRIDOR_EVERY = 2
/** Every how many reads the player's surroundings are read while no screen shows the map (every 6 s, to know
 * they're still on that server). */
const ALONE_EVERY = 2
/** The square round the player's truck (m either way) their server is looked for in, and always read. */
const AROUND_TRUCK = 1000
/** How long before a server is looked for again when the player's truck wasn't found on any. */
const LOOK_AGAIN_MS = 15000
/** How long before a server is looked for again when the player's truck fit on more than one (a truck of
 * another server where theirs is, as they stand: moving on tells them apart). */
const AMBIGUOUS_AGAIN_MS = 5000
/** How long (ms) of the player's own truck's path is kept, and how often a moment of it (ms). */
const PATH_KEPT_MS = 30000
const PATH_EVERY_MS = 250
/** How long (ms) the last read stands while the feed can't be reached: after it, no trucks are shown. */
const KEEP_LAST_READ_MS = 30000
/** Reads in a row the player's own truck may be missing (or far from theirs) before their server is looked
 * for again (they may have changed server). */
const SELF_MISSES = 3

interface TruckersMpState {
  /** The feed's map number of the player's server (`null`: not known). */
  server: number | null
  /** The player's own number on it. */
  self: number | null
  /** The feed's newest time in the last read of the part shown (s). */
  newest: number
  /** Each other truck in the part shown: its last places (the map draws it a few seconds behind, between
   * them: smooth, owner 2026-10-21); and how far the feed's clock is from the page's (s; `null`: not known). */
  tracks: Map<number, Sample[]>
  clock: number | null
  /** The other players along a route offered (its corridor), as last read. */
  along: OtherPlayer[]
  /** The other players along the route ahead while driving one (its corridor), as last read. */
  aheadAlong: OtherPlayer[]
  /** The part of the map a screen shows (`null`: none shows it), the corridor of a route offered, and that of
   * the route ahead while driving (read when no route is offered: the preview's traffic comes first). */
  view: Area | null
  corridor: Area | null
  ahead: Area | null
}

export const useTruckersMp = create<TruckersMpState>(() => ({
  server: null,
  self: null,
  newest: 0,
  tracks: new Map(),
  clock: null,
  along: [],
  aheadAlong: [],
  view: null,
  corridor: null,
  ahead: null,
}))

/** The part of the map a screen shows now (`null`: none shows it): the map tells it as it moves. */
export function setMapView(view: Area | null) {
  useTruckersMp.setState({ view })
}

/** The corridor of the route offered now, its traffic to read (`null`: none offered). */
export function setRouteCorridor(corridor: Area | null) {
  useTruckersMp.setState(corridor ? { corridor } : { corridor: null, along: [] })
}

/** The corridor of the route ahead while driving one, its traffic to read (`null`: no route, or not driving
 * one under TruckersMP). */
export function setRouteAhead(ahead: Area | null) {
  useTruckersMp.setState(ahead ? { ahead } : { ahead: null, aheadAlong: [] })
}

/** The smallest area holding both. */
function union(a: Area, b: Area): Area {
  return [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[2], b[2]), Math.max(a[3], b[3])]
}

async function read(url: string): Promise<unknown> {
  const answer = await fetch(url, { cache: 'no-store' })
  if (!answer.ok) throw new Error(`${answer.status}`)
  return answer.json()
}

/** The servers up for the game `game` (`null`: either's): each one's id (as TruckersMP's SDK names it) and its
 * map number in the feed. ETS2's ProMods servers are ETS2's (the list calls their game `promods`). */
async function servers(game: 'ets2' | 'ats' | null): Promise<{ id: number; map: number }[]> {
  const answer = (await read(SERVERS)) as {
    servers?: { id?: unknown; map?: unknown; status?: unknown; game?: unknown }[]
  }
  const games = game === 'ats' ? ['ats'] : game === 'ets2' ? ['ets2', 'promods'] : null
  return (answer.servers ?? [])
    .filter((server) => server.status === true && typeof server.id === 'number' && typeof server.map === 'number')
    .filter((server) => !games || (typeof server.game === 'string' && games.includes(server.game)))
    .map((server) => ({ id: server.id as number, map: server.map as number }))
}

/** The player's own truck's last moments, to tell it in the feed ([`selfOnPath`]). */
const path: PathPoint[] = []

/** Keeps a moment of the player's truck now and then, the last [`PATH_KEPT_MS`] of them. */
function keepPath() {
  const telemetry = useLive.getState().telemetry
  if (!telemetry) return
  const at = Date.now()
  const last = path.at(-1)
  if (last && at - last.at < PATH_EVERY_MS) return
  path.push({ at, x: telemetry.x, z: telemetry.z, heading: telemetry.heading })
  while (path.length && at - path[0].at > PATH_KEPT_MS) path.shift()
}

const readArea = async (map: number, area: Area) => readFeed(await read(`${FEED}?${areaQuery(area)}&server=${map}`))

function truckNow(): { x: number; z: number } | null {
  const telemetry = useLive.getState().telemetry
  return telemetry ? { x: telemetry.x, z: telemetry.z } : null
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/** Reads TruckersMP's other players while the game runs under it, from now on. Returns the function that
 * stops it. */
export function followTruckersMp(): () => void {
  let run = 0
  const start = () => {
    const mine = ++run
    void loop(() => mine === run)
  }
  const stop = () => {
    run++
    useTruckersMp.setState({ server: null, self: null, tracks: new Map(), along: [], aheadAlong: [] })
  }
  if (useLive.getState().truckersmp) start()
  const unsubscribe = useLive.subscribe((live, before) => {
    if (live.truckersmp && live.telemetry !== before.telemetry) keepPath()
    // Another game under TruckersMP (or its game said at last): its servers looked through again.
    if (live.truckersmp === before.truckersmp && live.truckersmpGame === before.truckersmpGame) return
    if (live.truckersmp) start()
    else stop()
  })
  return () => {
    unsubscribe()
    stop()
  }
}

/** The newest time the feed placed any of `read`'s trucks (s). */
function newestOf(read: readonly FeedPlayer[], fallback: number): number {
  return read.reduce((latest, player) => Math.max(latest, player.time), read.length ? -Infinity : fallback)
}

/** Finds the player's server, then reads what's shown every second, while `going()`. */
async function loop(going: () => boolean) {
  let misses = 0
  let tick = 0
  let lastRead = performance.now()
  const before = new Map<number, OtherPlayer>()
  const beforeAlong = new Map<number, OtherPlayer>()
  const beforeAhead = new Map<number, OtherPlayer>()
  const keep = (map: Map<number, OtherPlayer>, players: OtherPlayer[]) => {
    map.clear()
    for (const player of players) map.set(player.id, player)
  }
  // The server TruckersMP's SDK named, and its map number in the feed, as last worked out.
  let named: { id: number; map: number } | null = null
  while (going()) {
    const { server, self, view, corridor, ahead } = useTruckersMp.getState()
    const truck = truckNow()
    // The player's server and number from TruckersMP's SDK, when the plugin passes them.
    const sdk = useLive.getState().nearby
    try {
      // Another server than the one named: looked for again.
      if (sdk && server !== null && named && named.id !== sdk.server) {
        useTruckersMp.setState({ server: null, self: null, tracks: new Map(), along: [], aheadAlong: [] })
        continue
      }
      if (server === null) {
        let found: { server: number; self: number | null } | null = null
        let ambiguous = false
        const up = await servers(useLive.getState().truckersmpGame)
        if (!going()) return
        // Named by the SDK: no need to look for the player's truck.
        const match = sdk ? up.find((each) => each.id === sdk.server) : undefined
        if (sdk && match) {
          named = match
          // The player's number: the SDK's, once it has it (not yet as they join: worked out from each read).
          found = { server: match.map, self: sdk.local >= 0 ? sdk.local : null }
        } else if (truck && path.length) {
          // Every server of the game, for a truck that is the player's by its path: theirs only if on one alone.
          named = null
          const fits: { server: number; self: number }[] = []
          for (const { map } of up) {
            if (!going()) return
            const mine = selfOnPath(await readArea(map, around(truck, AROUND_TRUCK)), path)
            if (mine) fits.push({ server: map, self: mine.player.id })
          }
          if (fits.length === 1) found = fits[0]
          ambiguous = fits.length > 1
        }
        if (!going()) return
        if (!found) {
          await sleep(ambiguous ? AMBIGUOUS_AGAIN_MS : LOOK_AGAIN_MS)
          continue
        }
        useTruckersMp.setState(found)
        misses = 0
        continue
      }
      tick++
      // The part shown, with the player's surroundings (where their own truck is checked); none shown, only
      // the surroundings, now and then.
      if (truck && (view || tick % ALONE_EVERY === 0)) {
        const area = view ? union(view, around(truck, AROUND_TRUCK)) : around(truck, AROUND_TRUCK)
        const shown = await readArea(server, area)
        if (!going()) return
        // The player's own number, from this read (it may change as they join again), left out of the others.
        const others = sdk ? { local: sdk.local, others: sdk.players.map(([id]) => id) } : null
        const me = ownId(shown, truck, self, others)
        if (me !== self) useTruckersMp.setState({ self: me })
        // Still the player's server: named so by the SDK, or their own truck on it still where theirs was (its
        // path; a wrong one, not theirs, soon misses).
        const mine = shown.find((player) => player.id === me)
        const near = named !== null || (mine !== undefined && (path.length === 0 || pathDistance(mine, path) !== null))
        misses = near ? 0 : misses + 1
        if (misses >= SELF_MISSES) {
          before.clear()
          beforeAlong.clear()
          beforeAhead.clear()
          useTruckersMp.setState({ server: null, self: null, tracks: new Map(), along: [], aheadAlong: [] })
          continue
        }
        const known = useTruckersMp.getState()
        const newest = newestOf(shown, known.newest)
        const readAt = performance.now()
        useTruckersMp.setState({
          newest,
          tracks: withRead(known.tracks, shown, me),
          clock: shown.length ? feedClock(known.clock, newest - readAt / 1000) : known.clock,
        })
        lastRead = performance.now()
      }
      if (corridor && tick % CORRIDOR_EVERY === 0) {
        const along = otherPlayers(await readArea(server, corridor), useTruckersMp.getState().self, beforeAlong)
        if (!going()) return
        keep(beforeAlong, along)
        // Still the route offered (another may have been picked meanwhile).
        if (useTruckersMp.getState().corridor === corridor) useTruckersMp.setState({ along })
      } else if (!corridor && ahead && tick % CORRIDOR_EVERY === 0) {
        const aheadAlong = otherPlayers(await readArea(server, ahead), useTruckersMp.getState().self, beforeAhead)
        if (!going()) return
        keep(beforeAhead, aheadAlong)
        if (useTruckersMp.getState().ahead) useTruckersMp.setState({ aheadAlong })
      }
    } catch {
      // The feed unreachable for now: tried again next time, the last read kept a while meanwhile.
      if (performance.now() - lastRead > KEEP_LAST_READ_MS) {
        before.clear()
        useTruckersMp.setState({ tracks: new Map() })
      }
    }
    await sleep(READ_EVERY_MS)
  }
}
