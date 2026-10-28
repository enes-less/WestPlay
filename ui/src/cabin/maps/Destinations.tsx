import { useEffect, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { formatTripDistance, formatTripTime } from '../../shared/format'
import type { PlannedRoute, RouteAction, RouteAnswer } from '../../shared/host'
import { useKeyboard } from '../keyboard/store'
import { useLive } from '../state/store'
import { type Strings, useText } from '../state/text'
import { placesAhead } from './alongRoute'
import { liveRouteProgress, showOfferedRoutes, showTraffic } from './liveMap'
import { PlaceBadge } from './PlaceBadge'
import {
  type Destination,
  type DestinationKind,
  distanceTo,
  nameOf,
  nearestOfKind,
  ownGarages,
  searchPlaces,
  usePlaces,
} from './places'
import { toLngLat } from './projection'
import { useRecents } from './recents'
import { askDetours, changeRoute, followRoute, planRoutes } from './routeChange'
import { corridorOf, trafficAlong } from './traffic'
import { setRouteCorridor, useTruckersMp } from './truckersmp'

/** The kinds Find nearby offers, in its order. */
const NEARBY: DestinationKind[] = ['fuel', 'parking', 'service', 'hotel', 'company', 'dealer', 'ferry', 'city']

/** The game's map scale between cities (its world is this much smaller than the distances it gives), for
 * straight-line distances in the units its GPS uses (shared/format.ts). */
const MAP_SCALE = 19

/** How far the truck may move before the lists' distances are worked out again, metres. */
const WHERE_STEP = 250

/** Where the truck is (lng, lat), to the nearest `WHERE_STEP` (the lists don't redraw with every metre). */
function useTruckAt(): [number, number] | null {
  const at = useLive(
    useShallow((state) => {
      const t = state.telemetry
      return t ? [Math.round(t.x / WHERE_STEP) * WHERE_STEP, Math.round(t.z / WHERE_STEP) * WHERE_STEP] : null
    }),
  )
  return at ? toLngLat(at[0], at[1]) : null
}

/** What the game's route allows: on a job, only stops (the destination is the job's). */
interface RouteState {
  job: boolean
  route: boolean
}

function useRouteState(): RouteState {
  return useLive(useShallow((state) => ({ job: state.job !== null, route: state.route !== null })))
}

interface DestinationsProps {
  query: string
  /** The search field's id, to put the keyboard away when a list or a place is chosen. */
  field: string
  /** A route change the game made: what to tell the player (the panel closes). */
  onDone(message: string): void
}

/**
 * Destinations, under the search field (MapSearch.tsx): with nothing typed, the player's favourites (their
 * headquarters' garage, their other garages), Find nearby, and recent places; as they type, the places
 * matching. A place opens its card, whose button changes the game's own route (routeChange.ts): Go, or on a
 * route Add stop (a job's destination stays the job's).
 */
export function Destinations({ query, field, onDone }: DestinationsProps) {
  const text = useText()
  const places = usePlaces()
  const from = useTruckAt()
  // On a route, the stops it offers are along the rest of it, nearest along it first: never behind the truck
  // (PLAN.md: Navigation plan, A). Worked out again as the truck moves (`from` changes).
  const onRoute = useLive((state) => state.route !== null)
  const live = onRoute && places ? liveRouteProgress() : null
  const ahead = live && places ? placesAhead(places, live.spans, live.progress) : null
  const along = ahead ? new Map(ahead.map(({ place, along }) => [place, along])) : undefined
  const [kind, setKind] = useState<DestinationKind | null>(null)
  const [chosen, setChosen] = useState<Destination | null>(null)
  // Choosing a list or a place puts the keyboard away, as on the iPhone: what's chosen has the room.
  const choose = (place: Destination) => {
    useKeyboard.getState().detach(field)
    setChosen(place)
  }
  const open = (kind: DestinationKind) => {
    useKeyboard.getState().detach(field)
    setKind(kind)
  }

  // Which view shows, and how deep it is (Home or the results, a list, a place): a deeper one slides in from
  // the right, a shallower one from the left. The results change in place as the player types.
  const view = chosen ? `place:${chosen.node}` : query.trim() ? 'results' : kind ? `kind:${kind}` : 'home'
  const depth = chosen ? 2 : kind && !query.trim() ? 1 : 0
  const [shown, setShown] = useState({ view, depth, direction: '' })
  if (shown.view !== view) {
    const direction = depth > shown.depth ? 'forward' : depth < shown.depth ? 'back' : ''
    setShown({ view, depth, direction })
  }

  let content
  if (chosen) {
    content = (
      <PlaceCard place={chosen} from={from} along={along?.get(chosen)} onBack={() => setChosen(null)} onDone={onDone} />
    )
  } else if (places === undefined) {
    content = <p className="destinations-note">{text.destinations.loading}</p>
  } else if (places === null) {
    content = <p className="destinations-note">{text.destinations.noPlaces}</p>
  } else if (query.trim()) {
    const found = along
      ? searchPlaces([...along.keys()], query, from, text.destinations.kinds, (place) => along.get(place) ?? 0)
      : searchPlaces(places, query, from, text.destinations.kinds)
    content =
      found.length === 0 ? (
        <p className="destinations-note">{text.destinations.noResults}</p>
      ) : (
        <PlaceList places={found} from={from} along={along} onChoose={choose} />
      )
  } else if (kind) {
    content = (
      <>
        <button type="button" className="destinations-back" onClick={() => setKind(null)}>
          <BackGlyph />
          {text.destinations.nearby[kind]}
        </button>
        <PlaceList
          places={
            ahead
              ? ahead
                  .filter(({ place }) => place.kind === kind)
                  .map(({ place }) => place)
                  .slice(0, AHEAD_RESULTS)
              : nearestOfKind(places, kind, from)
          }
          from={from}
          along={along}
          onChoose={choose}
        />
      </>
    )
  } else {
    content = <Home places={places} from={from} onKind={open} onChoose={choose} onDone={onDone} />
  }
  return (
    <div key={view} className={`destinations-view ${shown.direction}`}>
      {content}
    </div>
  )
}

function Home({
  places,
  from,
  onKind,
  onChoose,
  onDone,
}: {
  places: Destination[]
  from: [number, number] | null
  onKind(kind: DestinationKind): void
  onChoose(place: Destination): void
  onDone(message: string): void
}) {
  const text = useText()
  const owned = useLive((state) => state.ownedGarages)
  const headquarters = useLive((state) => state.headquarters)
  const recents = useRecents((state) => state.places)
  const { job, route } = useRouteState()
  const garages = ownGarages(places, owned, headquarters)
  const [ending, setEnding] = useState<string | null>(null)

  return (
    <>
      {route && !job && (
        <button
          type="button"
          className="destinations-end"
          onClick={async () => {
            const answer = await changeRoute('end')
            if (answer === 'done') onDone(text.destinations.done.end)
            else setEnding(answerText(answer, text))
          }}
        >
          {text.destinations.endRoute}
        </button>
      )}
      {ending && <p className="destinations-note error">{ending}</p>}
      {garages.length > 0 && (
        <section>
          <h3>{text.destinations.favourites}</h3>
          {garages.map((garage) => (
            <PlaceRow
              key={garage.node}
              place={garage}
              title={garage.cityId === headquarters ? text.destinations.headquarters : text.destinations.yourGarage}
              from={from}
              onChoose={onChoose}
            />
          ))}
        </section>
      )}
      <section>
        <h3>{route ? text.destinations.alongRoute : text.destinations.findNearby}</h3>
        <div className="destinations-nearby">
          {NEARBY.map((kind) => (
            <button key={kind} type="button" onClick={() => onKind(kind)}>
              <PlaceBadge kind={kind} />
              <span>{text.destinations.nearby[kind]}</span>
            </button>
          ))}
        </div>
      </section>
      {recents.length > 0 && (
        <section>
          <h3>{text.destinations.recents}</h3>
          <PlaceList places={recents} from={from} onChoose={onChoose} />
        </section>
      )}
    </>
  )
}

/** What stopping at each of `places` would add to the trip (seconds on the map's ground), by place: asked
 * of the host once per set of places shown, on a route. */
function useDetours(places: Destination[], onRoute: boolean): Map<Destination, number> {
  const [found, setFound] = useState<{ key: string; seconds: Map<Destination, number> }>({
    key: '',
    seconds: new Map(),
  })
  const key = onRoute ? places.map((place) => place.node).join(',') : ''
  useEffect(() => {
    if (!key) return
    let current = true
    askDetours(places.map((place) => place.node)).then((seconds) => {
      if (!current) return
      const byPlace = new Map<Destination, number>()
      places.forEach((place, index) => {
        const added = seconds[index]
        if (added !== null && added !== undefined) byPlace.set(place, added)
      })
      setFound({ key, seconds: byPlace })
    })
    return () => {
      current = false
    }
    // `key` stands for the places: a new list of the same places asks nothing again.
  }, [key])
  return found.key === key ? found.seconds : new Map()
}

/** "+5 min": what a stop adds to the trip, in the game's scale. */
function detourText(seconds: number, text: Strings): string {
  return `+${formatTripTime(Math.max(1, (seconds * MAP_SCALE) / 60), text)}`
}

/** The most stops along the route a list offers. */
const AHEAD_RESULTS = 40

/** Places in a list; with `along` (on a route), each one's distance is how far along the route it is. */
function PlaceList({
  places,
  from,
  along,
  onChoose,
}: {
  places: Destination[]
  from: [number, number] | null
  along?: Map<Destination, number>
  onChoose(place: Destination): void
}) {
  const detours = useDetours(places, along !== undefined)
  return (
    <div className="destinations-list">
      {places.map((place, index) => (
        // Rows hold no state of their own: by position, whatever the places (two may share a node).
        <PlaceRow
          key={index}
          place={place}
          from={from}
          along={along?.get(place)}
          detour={detours.get(place)}
          onChoose={onChoose}
        />
      ))}
    </div>
  )
}

function PlaceRow({
  place,
  title,
  from,
  along,
  detour,
  onChoose,
}: {
  place: Destination
  title?: string
  from: [number, number] | null
  /** How far along the route it is (metres), on a route. */
  along?: number
  /** What stopping there adds to the trip (seconds on the map's ground), once known. */
  detour?: number
  onChoose(place: Destination): void
}) {
  const text = useText()
  const kindName = text.destinations.kinds[place.kind]
  const name = title ?? nameOf(place, kindName)
  // What it is and where, without saying either twice ("Garage · Bari", "Taranto · City"), and what it adds.
  const detail = [
    kindName !== name ? kindName : null,
    place.city !== name ? place.city : null,
    detour !== undefined ? detourText(detour, text) : null,
  ].filter(Boolean)
  return (
    <button type="button" className="destination-row" onClick={() => onChoose(place)}>
      <PlaceBadge kind={place.kind} />
      <span className="destination-text">
        <span className="destination-name">{name}</span>
        {detail.length > 0 && <span className="destination-detail">{detail.join(' · ')}</span>}
      </span>
      {along !== undefined ? (
        <span className="destination-distance">{formatTripDistance(along * MAP_SCALE, text.decimal)}</span>
      ) : (
        from && <span className="destination-distance">{distanceText(place, from, text)}</span>
      )}
    </button>
  )
}

function PlaceCard({
  place,
  from,
  along,
  onBack,
  onDone,
}: {
  place: Destination
  from: [number, number] | null
  /** How far along the route it is (metres), on a route. */
  along?: number
  onBack(): void
  onDone(message: string): void
}) {
  const text = useText()
  const remember = useRecents((state) => state.remember)
  const { job, route } = useRouteState()
  const [asking, setAsking] = useState(false)
  // Go first offers the routes there (Directions, as Apple Maps): WestPlay's own, the one picked then handed
  // to the game's GPS (PLAN.md: Navigation plan, B). `finding` while they're planned.
  const [directions, setDirections] = useState<
    { id: number; routes: PlannedRoute[]; picked: number } | 'finding' | null
  >(null)
  // The routes offered go from the map with the card, their traffic with them.
  useEffect(
    () => () => {
      showOfferedRoutes([], null)
      showTraffic([])
      setRouteCorridor(null)
    },
    [],
  )
  // Why the game didn't take it; each answer anew (a second press shows it again).
  const [problem, setProblem] = useState<{ text: string; id: number } | null>(null)
  const kindName = text.destinations.kinds[place.kind]
  const name = nameOf(place, kindName)
  // Without a job: Go, and on a route also Add stop; on a job only stops.
  const actions: ('go' | 'addStop')[] = job ? ['addStop'] : route ? ['go', 'addStop'] : ['go']

  const offer = async () => {
    setProblem(null)
    setDirections('finding')
    const { id, routes } = await planRoutes(place.node)
    // None (a map built before WestPlay planned routes, no way there): straight to the game, as before.
    if (routes.length === 0) {
      setDirections(null)
      await ask('go')
      return
    }
    setDirections({ id, routes, picked: 0 })
  }
  const pick = (index: number) =>
    setDirections((shown) => (typeof shown === 'object' && shown !== null ? { ...shown, picked: index } : shown))
  // On the map as they're offered or another is picked, once the card has its narrower Directions width (the
  // map frames them beside it); a line tapped there picks it too.
  useEffect(() => {
    if (typeof directions !== 'object' || directions === null) return
    showOfferedRoutes(
      directions.routes.map((route) => route.line),
      directions.picked,
      pick,
    )
  }, [directions])
  // TruckersMP: the traffic on the route previewed, from the other players along it (its corridor read every
  // few seconds), anew with each read of them.
  useEffect(() => {
    if (typeof directions !== 'object' || directions === null) return
    setRouteCorridor(corridorOf(directions.routes[directions.picked]?.line ?? []))
  }, [directions])
  const playersAlong = useTruckersMp((state) => state.along)
  useEffect(() => {
    if (typeof directions !== 'object' || directions === null) return
    showTraffic(trafficAlong(directions.routes[directions.picked]?.line ?? [], playersAlong))
  }, [directions, playersAlong])
  const start = async () => {
    if (typeof directions !== 'object' || directions === null) return
    setAsking(true)
    setProblem(null)
    const { follows, answer } = await followRoute(directions.id, directions.picked)
    setAsking(false)
    if (follows || answer === 'ownWay') {
      showOfferedRoutes([], null)
      showTraffic([])
      setRouteCorridor(null)
      remember(place)
      onDone(follows ? text.destinations.done.go : text.destinations.ownWay)
    } else {
      setProblem((last) => ({ text: answerText(answer as RouteAnswer, text), id: (last?.id ?? 0) + 1 }))
    }
  }

  const ask = async (action: RouteAction) => {
    setAsking(true)
    setProblem(null)
    // The route's distance and time left before a stop: what the stop adds is the difference after.
    const before = useLive.getState().telemetry
    const answer = await changeRoute(action, place.node)
    if (answer === 'done' && action === 'addStop' && before && before.navigationDistance > 0) {
      const added = await stopAdded(place, before, text)
      setAsking(false)
      remember(place)
      onDone(added)
      return
    }
    setAsking(false)
    if (answer === 'done') {
      remember(place)
      onDone(text.destinations.done[action])
    } else {
      setProblem((last) => ({ text: answerText(answer, text), id: (last?.id ?? 0) + 1 }))
    }
  }

  return (
    <div className="destination-card">
      <button type="button" className="destinations-back" onClick={onBack}>
        <BackGlyph />
        {text.destinations.back}
      </button>
      <div className="destination-card-head">
        <PlaceBadge kind={place.kind} />
        <div>
          <h2>{name}</h2>
          <p>
            {[kindName !== name ? kindName : null, place.city !== name ? place.city : null].filter(Boolean).join(' · ')}
          </p>
        </div>
      </div>
      {along !== undefined ? (
        <p className="destination-away">
          {text.destinations.alongTheRoute(formatTripDistance(along * MAP_SCALE, text.decimal))}
        </p>
      ) : (
        from && <p className="destination-away">{text.destinations.straightLine(distanceText(place, from, text))}</p>
      )}
      {problem && (
        <p key={problem.id} className="destination-problem" role="status">
          {problem.text}
        </p>
      )}
      {directions === 'finding' && <p className="destination-away">{text.destinations.finding}</p>}
      {typeof directions === 'object' && directions !== null && (
        // The route in focus carries Go in its own row (owner 2026-10-21), moving to whichever is previewed.
        <div className="directions">
          {directions.routes.map((route, index) => (
            <div key={index} className="direction" data-picked={index === directions.picked || undefined}>
              <button type="button" className="direction-pick" onClick={() => pick(index)}>
                <span className="direction-time">{formatTripTime((route.seconds * MAP_SCALE) / 60, text)}</span>
                <span className="direction-detail">
                  {text.destinations.plans[route.plan]} · {formatTripDistance(route.length * MAP_SCALE, text.decimal)}
                </span>
              </button>
              {index === directions.picked && (
                <button type="button" className="direction-go" disabled={asking} onClick={start}>
                  {text.destinations.go}
                </button>
              )}
            </div>
          ))}
        </div>
      )}
      {!(typeof directions === 'object' && directions !== null) && (
        <div className="destination-actions">
          {actions.map((action, index) => (
            <button
              key={action}
              type="button"
              className={index === 0 ? 'destination-action primary' : 'destination-action'}
              disabled={asking || directions === 'finding'}
              onClick={() => (action === 'go' ? offer() : ask(action))}
            >
              {text.destinations[action]}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

/** How far a place is in a straight line, in the units the game's GPS gives distances in. */
function distanceText(place: Destination, from: [number, number], text: Strings): string {
  return formatTripDistance(distanceTo(place, from) * MAP_SCALE, text.decimal)
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/**
 * Once the game has a stop: waits (a few seconds at most) for its new route, then says what the stop adds
 * (the game's own distance and time left, after less before), or that the new route doesn't pass it.
 */
async function stopAdded(
  place: Destination,
  before: { navigationDistance: number; navigationTime: number },
  text: Strings,
): Promise<string> {
  const plain = text.destinations.done.addStop
  const old = useLive.getState().route
  const until = performance.now() + 5000
  while (useLive.getState().route === old && performance.now() < until) await sleep(200)
  if (useLive.getState().route === old) return plain
  // The map draws the new route on its next frame, and the game's distance left follows within a second.
  await sleep(1200)
  const live = liveRouteProgress()
  if (live && placesAhead([place], live.spans, live.progress).length === 0) return text.destinations.notOnRoute
  const after = useLive.getState().telemetry
  if (!after || after.navigationDistance <= 0) return plain
  const metres = after.navigationDistance - before.navigationDistance
  const minutes = (after.navigationTime - before.navigationTime) / 60
  if (metres <= 0) return plain
  return text.destinations.stopAdds(
    formatTripDistance(metres, text.decimal),
    formatTripTime(Math.max(1, minutes), text),
  )
}

/** What the game's answer means for the player, when it wasn't done. */
function answerText(answer: RouteAnswer, text: Strings): string {
  switch (answer) {
    case 'alreadyThere':
      return text.destinations.alreadyThere
    case 'full':
      return text.destinations.full
    case 'onJob':
      return text.destinations.onJob
    case 'unsupported':
      return text.destinations.unsupported
    case 'noAnswer':
      return text.destinations.noAnswer
    case 'elevated':
      return text.destinations.elevated
    case 'noSuchNode':
    case 'unexpected':
    case 'done':
      return text.destinations.failed
  }
}

function BackGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M15 5 8 12l7 7" />
    </svg>
  )
}
