import { useEffect, useState } from 'react'
import { SearchGlyph } from '../keyboard/glyphs'
import { useKeyboard } from '../keyboard/store'
import { TextField } from '../keyboard/TextField'
import { useLive } from '../state/store'
import { useText } from '../state/text'
import { Destinations } from './Destinations'
import { guidanceOf } from './guidance'

/** How long the word that the game took a change stays over the map, ms. */
const CONFIRMATION_MS = 2500
/** Must match the panel's closing animation in maps.css: how long it takes to shrink back, ms. */
const CLOSING_MS = 280
const FIELD = 'map-search'

/**
 * Maps' search, always at hand: a button over the map's top left, where CarPlay keeps it, or its top right
 * while a route is guided (the turn-by-turn banner has the left), since a route can still take more stops
 * (the game's waypoints, put in order by the game). It opens Destinations (Destinations.tsx) under the search
 * field, selected, the keyboard up: the panel grows out of the button, and shrinks back into it when it closes.
 * A change the game takes closes it, with a word over the map.
 */
export function MapSearch() {
  const text = useText()
  const [phase, setPhase] = useState<'closed' | 'open' | 'closing'>('closed')
  const [query, setQuery] = useState('')
  const [confirmation, setConfirmation] = useState<{ text: string; id: number } | null>(null)
  const typing = useKeyboard((state) => state.field?.id === FIELD)
  const guided = useLive(({ route, routeNext, telemetry }) => {
    return (
      route !== null &&
      guidanceOf(route.distances, route.maneuvers, routeNext, telemetry?.navigationDistance ?? 0) !== null
    )
  })

  useEffect(() => {
    if (!confirmation) return
    const timer = setTimeout(() => setConfirmation(null), CONFIRMATION_MS)
    return () => clearTimeout(timer)
  }, [confirmation])

  useEffect(() => {
    if (phase !== 'closing') return
    const timer = setTimeout(() => {
      setPhase('closed')
      setQuery('')
    }, CLOSING_MS)
    return () => clearTimeout(timer)
  }, [phase])

  const close = () => {
    useKeyboard.getState().detach(FIELD)
    setPhase('closing')
  }
  // Grows from where its button is: the map's top left, or top right beside the turn-by-turn banner.
  const side = guided ? ' right' : ''

  return (
    <>
      {phase !== 'closed' ? (
        <div className={`map-search glass${side}${typing ? ' typing' : ''}${phase === 'closing' ? ' closing' : ''}`}>
          <div className="map-search-top">
            <TextField
              id={FIELD}
              value={query}
              onChange={setQuery}
              returnKind="search"
              hint={text.searchPlaces}
              label={text.search}
              icon={<SearchGlyph />}
              selectOnShow={phase === 'open'}
            />
            <button type="button" className="map-search-close" aria-label={text.close} onClick={close}>
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="m7 7 10 10M17 7 7 17" />
              </svg>
            </button>
          </div>
          <div className="destinations">
            <Destinations
              query={query}
              field={FIELD}
              onDone={(message) => {
                close()
                setConfirmation({ text: message, id: Date.now() })
              }}
            />
          </div>
        </div>
      ) : (
        <button
          type="button"
          className={guided ? 'map-search-button glass guided' : 'map-search-button glass'}
          aria-label={text.search}
          onClick={() => setPhase('open')}
        >
          <SearchGlyph />
        </button>
      )}
      {confirmation && (
        <div key={confirmation.id} className="map-confirmation glass" role="status">
          {confirmation.text}
        </div>
      )}
    </>
  )
}
