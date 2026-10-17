import { useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { formatTripDistance } from '../../shared/format'
import type { RouteAction, RouteAnswer } from '../../shared/host'
import { useKeyboard } from '../keyboard/store'
import { useLive } from '../state/store'
import { type Strings, useText } from '../state/text'
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
import { changeRoute } from './routeChange'

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
    content = <PlaceCard place={chosen} from={from} onBack={() => setChosen(null)} onDone={onDone} />
  } else if (places === undefined) {
    content = <p className="destinations-note">{text.destinations.loading}</p>
  } else if (places === null) {
    content = <p className="destinations-note">{text.destinations.noPlaces}</p>
  } else if (query.trim()) {
    const found = searchPlaces(places, query, from, text.destinations.kinds)
    content =
      found.length === 0 ? (
        <p className="destinations-note">{text.destinations.noResults}</p>
      ) : (
        <PlaceList places={found} from={from} onChoose={choose} />
      )
  } else if (kind) {
    content = (
      <>
        <button type="button" className="destinations-back" onClick={() => setKind(null)}>
          <BackGlyph />
          {text.destinations.nearby[kind]}
        </button>
        <PlaceList places={nearestOfKind(places, kind, from)} from={from} onChoose={choose} />
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
        <h3>{text.destinations.findNearby}</h3>
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

function PlaceList({
  places,
  from,
  onChoose,
}: {
  places: Destination[]
  from: [number, number] | null
  onChoose(place: Destination): void
}) {
  return (
    <div className="destinations-list">
      {places.map((place, index) => (
        // Rows hold no state of their own: by position, whatever the places (two may share a node).
        <PlaceRow key={index} place={place} from={from} onChoose={onChoose} />
      ))}
    </div>
  )
}

function PlaceRow({
  place,
  title,
  from,
  onChoose,
}: {
  place: Destination
  title?: string
  from: [number, number] | null
  onChoose(place: Destination): void
}) {
  const text = useText()
  const kindName = text.destinations.kinds[place.kind]
  const name = title ?? nameOf(place, kindName)
  // What it is and where, without saying either twice ("Garage · Bari", "Taranto · City").
  const detail = [kindName !== name ? kindName : null, place.city !== name ? place.city : null].filter(Boolean)
  return (
    <button type="button" className="destination-row" onClick={() => onChoose(place)}>
      <PlaceBadge kind={place.kind} />
      <span className="destination-text">
        <span className="destination-name">{name}</span>
        {detail.length > 0 && <span className="destination-detail">{detail.join(' · ')}</span>}
      </span>
      {from && <span className="destination-distance">{distanceText(place, from, text)}</span>}
    </button>
  )
}

function PlaceCard({
  place,
  from,
  onBack,
  onDone,
}: {
  place: Destination
  from: [number, number] | null
  onBack(): void
  onDone(message: string): void
}) {
  const text = useText()
  const remember = useRecents((state) => state.remember)
  const { job, route } = useRouteState()
  const [asking, setAsking] = useState(false)
  // Why the game didn't take it; each answer anew (a second press shows it again).
  const [problem, setProblem] = useState<{ text: string; id: number } | null>(null)
  const kindName = text.destinations.kinds[place.kind]
  const name = nameOf(place, kindName)
  // Without a job: Go, and on a route also Add stop; on a job only stops.
  const actions: ('go' | 'addStop')[] = job ? ['addStop'] : route ? ['go', 'addStop'] : ['go']

  const ask = async (action: RouteAction) => {
    setAsking(true)
    setProblem(null)
    const answer = await changeRoute(action, place.node)
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
      {from && <p className="destination-away">{text.destinations.straightLine(distanceText(place, from, text))}</p>}
      {problem && (
        <p key={problem.id} className="destination-problem" role="status">
          {problem.text}
        </p>
      )}
      <div className="destination-actions">
        {actions.map((action, index) => (
          <button
            key={action}
            type="button"
            className={index === 0 ? 'destination-action primary' : 'destination-action'}
            disabled={asking}
            onClick={() => ask(action)}
          >
            {text.destinations[action]}
          </button>
        ))}
      </div>
    </div>
  )
}

/** How far a place is in a straight line, in the units the game's GPS gives distances in. */
function distanceText(place: Destination, from: [number, number], text: Strings): string {
  return formatTripDistance(distanceTo(place, from) * MAP_SCALE, text.decimal)
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
