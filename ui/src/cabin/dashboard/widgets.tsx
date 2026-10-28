import { type Backdrop, type Look, useWordsOver, wordsClass } from '../apps/music/contrast'
import { Crossfade } from '../apps/music/Crossfade'
import { VideoWidget } from '../apps/youtube/VideoWidget'
import { NextGlyph, NoteGlyph, PauseGlyph, PlayGlyph, PreviousGlyph } from '../apps/music/glyphs'
import { TrackSlide } from '../apps/music/TrackSlide'
import { useTrackTransition } from '../apps/music/useTrackTransition'
import { playOrPause, skip, useSpotify, useSpotifySource } from '../apps/spotify/spotify'
import {
  playOrPause as playOrPauseAppleMusic,
  skip as skipAppleMusic,
  useAppleMusic,
  useAppleMusicSource,
} from '../apps/music/appleMusic'
import {
  playOrPause as playOrPauseYouTubeMusic,
  skip as skipYouTubeMusic,
  useYouTubeMusic,
  useYouTubeMusicSource,
} from '../apps/youtubeMusic/youtubeMusic'
import type { MediaAction, MediaSource } from '../../shared/media'
import { useLook } from '../state/hooks'
import { go, useCabin } from '../state/store'
import { useMapData } from '../maps/mapData'
import { useLive } from '../state/store'
import { useText } from '../state/text'
import { nearestCity } from './destination'
import { useNowPlaying } from './useNowPlaying'
import { weatherView } from './weather'
import { WeatherGlyph } from './WeatherGlyph'
import { EMPTY, useDestinationView, useGuided } from './widgetState'
import { cabClock } from '../../shared/host'

/**
 * Now Playing: what plays, the video YouTube plays first (VideoWidget), then the Spotify app's music, then
 * Apple Music's (the Music app), then YouTube Music's; nothing playing, what played last (nowPlaying.ts); none
 * of them with anything, Not Playing. Turned off in Settings, its room goes to the others.
 */
export function NowPlayingWidget() {
  const shown = useCabin((state) => state.dashboardWidgets.nowPlaying)
  const { showing, spotify, appleMusic, youtubeMusic } = useNowPlaying()
  return (
    <div className={`widget-slot${shown ? ' open' : ''}`}>
      {shown && showing === 'video' && <VideoWidget />}
      {shown && showing === 'spotify' && spotify && <SpotifyWidget spotify={spotify} />}
      {shown && showing === 'appleMusic' && appleMusic && <AppleMusicWidget appleMusic={appleMusic} />}
      {shown && showing === 'youtubeMusic' && youtubeMusic && <YouTubeMusicWidget youtubeMusic={youtubeMusic} />}
      {shown && showing === null && <MusicWidget source={null} artwork={null} onControl={() => {}} />}
    </div>
  )
}

/** Apple Music's music (the Music app's): what plays, its controls, opening Music on Now Playing. */
function AppleMusicWidget({ appleMusic }: { appleMusic: NonNullable<ReturnType<typeof useAppleMusicSource>> }) {
  return (
    <MusicWidget
      source={appleMusic.source}
      artwork={appleMusic.artwork}
      onOpen={() => {
        useAppleMusic.setState({ nowPlaying: true })
        go({ type: 'openApp', app: 'music' })
      }}
      onControl={(action) => {
        if (action === 'playPause') playOrPauseAppleMusic()
        else if (action === 'next') skipAppleMusic(1)
        else if (action === 'previous') skipAppleMusic(-1)
      }}
    />
  )
}

/** YouTube Music's music: what plays, its controls, opening YouTube Music on Now Playing. */
function YouTubeMusicWidget({ youtubeMusic }: { youtubeMusic: NonNullable<ReturnType<typeof useYouTubeMusicSource>> }) {
  return (
    <MusicWidget
      source={youtubeMusic.source}
      artwork={youtubeMusic.artwork}
      onOpen={() => {
        useYouTubeMusic.setState({ nowPlaying: true })
        go({ type: 'openApp', app: 'youtubeMusic' })
      }}
      onControl={(action) => {
        if (action === 'playPause') playOrPauseYouTubeMusic()
        else if (action === 'next') skipYouTubeMusic(1)
        else if (action === 'previous') skipYouTubeMusic(-1)
      }}
    />
  )
}

/** The Spotify app's music: what Spotify plays, its controls, opening the Spotify app on Now Playing. */
function SpotifyWidget({ spotify }: { spotify: NonNullable<ReturnType<typeof useSpotifySource>> }) {
  return (
    <MusicWidget
      source={spotify.source}
      artwork={spotify.artwork}
      onOpen={() => {
        useSpotify.setState({ nowPlaying: true })
        go({ type: 'openApp', app: 'spotify' })
      }}
      onControl={(action) => {
        if (action === 'playPause') playOrPause()
        else if (action === 'next') skip(1)
        else if (action === 'previous') skip(-1)
      }}
    />
  )
}

/** Behind Now Playing's words, in each look: the blurred cover at its opacity (dashboard.css) over the look's
 * glass, on the page behind an app (what's under the glass, roughly). */
const WIDGET_BACKDROPS: Record<Look, Backdrop> = {
  dark: { cover: 0.55, rest: [40, 40, 42] },
  light: { cover: 0.55, rest: [248, 248, 250] },
}

/**
 * Now Playing's music: a source's track, tinted by its album art (a blurred copy behind the glass). Track
 * changes animate (the new cover waited for, the text sliding, cover and tint crossfading). The track opens
 * its app (if WestPlay has one for it); the controls work in place, on what's playing now. Over a cover the
 * look's words wouldn't read on, the other look's (a light cover: dark words); the glass and tint stay.
 */
function MusicWidget({
  source,
  artwork,
  onOpen,
  onControl,
}: {
  source: MediaSource | null
  artwork: string | null
  onOpen?(): void
  onControl(action: MediaAction): void
}) {
  const transition = useTrackTransition(source, artwork)
  const albumArt = useCabin((state) => state.albumArt)
  const cover = albumArt ? (transition.shown?.artwork ?? null) : null
  const controls = source?.controls
  const playing = source?.status === 'playing'
  const text = useText()
  const ink = useWordsOver(cover, useLook().appearance, WIDGET_BACKDROPS)

  return (
    <article className={`widget glass now-playing ${wordsClass(ink)}`}>
      <Crossfade value={cover} className="np-tint" render={(url) => url && <img src={url} alt="" aria-hidden />} />
      <button type="button" className="np-top" onClick={onOpen} disabled={!onOpen}>
        <Crossfade
          value={cover}
          className="art"
          render={(url) =>
            url ? (
              <img src={url} alt="" />
            ) : (
              <span className="art-placeholder" aria-hidden>
                <NoteGlyph size={36} />
              </span>
            )
          }
        />
        <TrackSlide
          transition={transition}
          className="track"
          layerClassName="track-lines"
          render={(track) => (
            <>
              <span className="title">{track?.source.title ?? text.notPlaying}</span>
              {track && <span className="artist">{track.source.artist}</span>}
            </>
          )}
        />
      </button>
      <div className="controls">
        <button
          type="button"
          aria-label={text.previous}
          disabled={!controls?.previous}
          onClick={() => onControl('previous')}
        >
          <PreviousGlyph size={34} />
        </button>
        <button
          type="button"
          aria-label={playing ? text.pause : text.play}
          disabled={!controls?.playPause}
          onClick={() => onControl('playPause')}
        >
          {playing ? <PauseGlyph size={40} /> : <PlayGlyph size={40} />}
        </button>
        <button type="button" aria-label={text.next} disabled={!controls?.next} onClick={() => onControl('next')}>
          <NextGlyph size={34} />
        </button>
      </div>
    </article>
  )
}

/**
 * Where the truck is headed, our "live activity" (destination.ts): the job's pickup and then its delivery
 * (deadline and distance left), or where the game's route ends; folded away without either, as the
 * guidance card comes and goes.
 */
export function DestinationWidget() {
  const view = useDestinationView()
  const shown = useCabin((state) => state.dashboardWidgets.destination) && view !== EMPTY
  return (
    <div className={`widget-slot${shown ? ' open' : ''}`}>
      {shown && (
        <article className="widget glass job">
          <div className="kicker">{view.kicker}</div>
          <div className="destination">{view.title}</div>
          {view.subtitle && <div className="place">{view.subtitle}</div>}
          <div className={`deadline${view.late ? ' late' : ''}`}>{view.detail}</div>
        </article>
      )}
    </div>
  )
}

/** How many widgets the column holds at most: the weather comes in only where there's room. */
const COLUMN = 3

/**
 * The weather where the truck is, as CarPlay's weather widget (weather.ts): the nearest city, the weather
 * and its symbol. The column holds three widgets: it comes in where there's room (turn-by-turn, Now Playing
 * or the destination not showing: no route, no job, or turned off in Settings), as they come and go; and
 * not without the weather (no game, or one whose weather isn't read), or turned off itself.
 */
export function WeatherWidget() {
  const text = useText()
  const labels = useMapData()?.labels
  const widgets = useCabin((state) => state.dashboardWidgets)
  const guided = useGuided()
  const destination = useDestinationView()
  const others = [guided, widgets.nowPlaying, widgets.destination && destination !== EMPTY].filter(Boolean).length
  const room = widgets.weather && others < COLUMN
  const weather = useLive((state) => state.weather)
  const gameTime = useLive((state) => (state.telemetry ? cabClock(state.telemetry, state.clockShift) : 0))
  const place = useLive((state) => {
    const t = state.telemetry
    return t && labels ? nearestCity([t.x, t.z], labels) : null
  })
  const view = weather ? weatherView(weather, gameTime, place, text) : null
  return (
    <div className={`widget-slot${room && view ? ' open' : ''}`}>
      {view && (
        <article className="widget glass weather" aria-label={[view.place, view.condition].filter(Boolean).join(', ')}>
          <div className="weather-text">
            {view.place && <div className="place">{view.place}</div>}
            <div className="condition">{view.condition}</div>
          </div>
          <WeatherGlyph symbol={view.symbol} size={84} />
        </article>
      )}
    </div>
  )
}
