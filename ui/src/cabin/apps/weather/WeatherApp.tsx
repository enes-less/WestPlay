import type { ReactNode } from 'react'
import { formatGameTime, formatTripTime } from '../../../shared/format'
import { nearestCity } from '../../dashboard/destination'
import { isNight, nextChange, roads, weatherView } from '../../dashboard/weather'
import { WeatherGlyph } from '../../dashboard/WeatherGlyph'
import { useMapData } from '../../maps/mapData'
import { useLive } from '../../state/store'
import { useText } from '../../state/text'
import './weather.css'
import { cabClock } from '../../../shared/host'

/** The sky behind the app: the weather's, by day or by night. */
type Sky = 'day' | 'night' | 'rain-day' | 'rain-night'

const SKIES: readonly Sky[] = ['day', 'night', 'rain-day', 'rain-night']

/**
 * The Weather app, in Apple's Weather look: the sky of the moment behind everything (rain falling in it while
 * it rains), where the truck is and the weather there at the top, and cards below for what the game tells of
 * what's to come: when it changes its weather next (game time), and how wet the roads are. The game has no
 * temperature, so its weather's symbol stands where Apple's app has the temperature.
 */
export function WeatherApp() {
  const text = useText()
  const weather = useLive((state) => state.weather)
  // Changes once a game minute: the clock of the next change, and night or day.
  const gameTime = useLive((state) => (state.telemetry ? cabClock(state.telemetry, state.clockShift) : null))
  const labels = useMapData()?.labels
  const place = useLive((state) => {
    const t = state.telemetry
    return t && labels ? nearestCity([t.x, t.z], labels) : null
  })

  if (!weather || gameTime === null) {
    return (
      <section className="app-panel weather-app" aria-label={text.apps.weather}>
        <SkyLayers sky="night" rain={false} />
        <p className="wx-unavailable">{text.weatherApp.unavailable}</p>
      </section>
    )
  }

  const view = weatherView(weather, gameTime, place, text)
  const rain = weather.now === 'rain' || weather.coming === 'rain'
  const night = isNight(gameTime)
  const sky: Sky = rain ? (night ? 'rain-night' : 'rain-day') : night ? 'night' : 'day'
  const next = nextChange(gameTime, weather.changeIn)
  const road = typeof weather.wetness === 'number' ? roads(weather.wetness, weather.now === 'rain') : null

  return (
    <section className="app-panel weather-app" aria-label={text.apps.weather}>
      <SkyLayers sky={sky} rain={weather.now === 'rain'} />
      <div className="wx-content">
        <header className="wx-hero">
          {place && <div className="wx-label">{text.weatherApp.myLocation}</div>}
          <h1 className="wx-place">{place ?? text.weatherApp.myLocation}</h1>
          <WeatherGlyph symbol={view.symbol} size={156} />
          <div className="wx-condition">{view.condition}</div>
        </header>
        <div className="wx-cards">
          {next && (
            <Card icon={<ClockGlyph />} title={text.weatherApp.nextChange}>
              <div className="wx-value">{formatGameTime(Math.round(next.at))}</div>
              <div className="wx-note">{text.weatherApp.inAbout(formatTripTime(next.in, text))}</div>
            </Card>
          )}
          {road && (
            <Card icon={<DropGlyph />} title={text.weatherApp.roads}>
              <div className="wx-value">{text.weatherApp.roadStates[road.state]}</div>
              <div className="wx-meter" aria-hidden>
                <span style={{ width: `${road.wetness}%` }} />
              </div>
              {road.trend && <div className="wx-note">{text.weatherApp.roadTrends[road.trend]}</div>}
            </Card>
          )}
        </div>
      </div>
    </section>
  )
}

/** Every sky, the one of the moment shown: a change fades from one to the next. Rain falls over it. */
function SkyLayers({ sky, rain }: { sky: Sky; rain: boolean }) {
  return (
    <div className="wx-sky" aria-hidden>
      {SKIES.map((each) => (
        <div key={each} className={`wx-sky-layer ${each}${each === sky ? ' shown' : ''}`} />
      ))}
      <div className={rain ? 'wx-rain shown' : 'wx-rain'}>
        <div className="wx-rain-fall" />
      </div>
    </div>
  )
}

/** A card on the sky, as Apple's: its title small above, with a symbol, and its content. */
function Card({ icon, title, children }: { icon: ReactNode; title: string; children: ReactNode }) {
  return (
    <article className="wx-card">
      <h2 className="wx-card-title">
        {icon}
        {title}
      </h2>
      {children}
    </article>
  )
}

/** Our own small symbols for the cards' titles. */
function ClockGlyph() {
  return (
    <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
      <circle cx="10" cy="10" r="7.5" />
      <path d="M10 5.5V10l3 2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function DropGlyph() {
  return (
    <svg viewBox="0 0 20 20" width="18" height="18" fill="currentColor" aria-hidden>
      <path d="M10 2.2c-.3 0-.5.2-.7.5C7.6 5.3 4.5 9.4 4.5 12.3a5.5 5.5 0 0 0 11 0c0-2.9-3.1-7-4.8-9.6-.2-.3-.4-.5-.7-.5Z" />
    </svg>
  )
}
