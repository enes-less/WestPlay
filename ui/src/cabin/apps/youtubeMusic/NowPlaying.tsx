import { type CSSProperties, type PointerEvent, useRef, useState } from 'react'
import { useText } from '../../state/text'
import { useCoverColor } from './coverColor'
import {
  AlbumGlyph,
  ArtistGlyph,
  ChevronDownGlyph,
  EqualizerGlyph,
  MoreGlyph,
  NextGlyph,
  PauseGlyph,
  PlayGlyph,
  PlayNextGlyph,
  PreviousGlyph,
  QueueGlyph,
  RadioGlyph,
  RepeatGlyph,
  ShuffleGlyph,
  ThumbDownGlyph,
  ThumbUpGlyph,
  VolumeGlyph,
} from './glyphs'
import { Cover, Failed, Section } from './parts'
import { VideoPicture } from './VideoPicture'
import {
  type Panel,
  type Playback,
  type YouTubeMusicItem,
  cycleRepeat,
  jumpTo,
  openPage,
  playOrPause,
  positionOf,
  queueSong,
  rate,
  seek,
  setVideo,
  setVolume,
  skip,
  startRadio,
  toggleShuffle,
  useAsked,
  useNow,
  useRating,
  useYouTubeMusic,
} from './youtubeMusic'

/** "3:08" */
function clock(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds))
  const hours = Math.floor(whole / 3600)
  const minutes = Math.floor((whole % 3600) / 60)
  const rest = String(whole % 60).padStart(2, '0')
  return hours ? `${hours}:${String(minutes).padStart(2, '0')}:${rest}` : `${minutes}:${rest}`
}

/** The song's time now, ticking while it plays. */
function usePosition(playback: Playback): number {
  const now = useNow(playback.playing && !playback.loading)
  return positionOf(playback, now)
}

/** The song playing, as a song's row is (for its ⋮ menu). */
function itemOf(playback: Playback): YouTubeMusicItem {
  const track = playback.track
  return {
    kind: track.kind,
    title: track.title,
    subtitle: track.artists,
    ...(track.image ? { image: track.image } : {}),
    ...(track.video ? { video: track.video } : {}),
    ...(track.artist ? { artist: track.artist } : {}),
    ...(track.albumPage ? { album: track.albumPage } : {}),
  }
}

/** The mini player, as YouTube Music's iPad app has it: a slim bar over the tabs, the song and play or pause;
 * pressed, Now Playing. Its thin line of how far the song is breathes while the song waits for its sound. */
export function MiniPlayer() {
  const text = useText()
  const playback = useYouTubeMusic((state) => state.playback)
  if (!playback) return null
  const words = text.youtubeMusic
  return (
    <div
      className="ytm-mini"
      role="button"
      tabIndex={0}
      aria-label={words.nowPlaying}
      onClick={() => useYouTubeMusic.setState({ nowPlaying: true })}
    >
      <MiniLine playback={playback} />
      <Cover
        item={{ title: playback.track.title, kind: 'song', image: playback.track.image }}
        className="ytm-mini-cover"
      />
      <span className="ytm-mini-text">
        <span className="ytm-mini-title">{playback.track.title}</span>
        <span className="ytm-mini-sub">{playback.ad ? words.ad : playback.track.artists}</span>
      </span>
      <button
        type="button"
        className="ytm-round"
        aria-label={playback.playing ? words.pause : words.play}
        onClick={(event) => {
          event.stopPropagation()
          playOrPause()
        }}
      >
        {playback.playing ? <PauseGlyph size={32} /> : <PlayGlyph size={32} />}
      </button>
    </div>
  )
}

function MiniLine({ playback }: { playback: Playback }) {
  const position = usePosition(playback)
  const share = playback.duration > 0 ? Math.min(1, position / playback.duration) : 0
  return (
    <span className={playback.loading ? 'ytm-mini-line loading' : 'ytm-mini-line'}>
      <span style={{ transform: `scaleX(${share})` }} />
    </span>
  )
}

/** A thin line with a dot, to drag along (how far the song is; the volume): `share` of it filled, said as it's
 * dragged (`onMove`) and as it's let go (`onDone`). */
function Slider({
  share,
  label,
  className = '',
  onMove,
  onDone,
}: {
  share: number
  label: string
  className?: string
  onMove?(share: number): void
  onDone(share: number): void
}) {
  const bar = useRef<HTMLDivElement>(null)
  const [dragged, setDragged] = useState<number | null>(null)
  const shown = Math.min(1, Math.max(0, dragged ?? share))
  const at = (event: PointerEvent) => {
    const box = bar.current?.getBoundingClientRect()
    if (!box || box.width === 0) return 0
    return Math.min(1, Math.max(0, (event.clientX - box.left) / box.width))
  }
  return (
    <div
      ref={bar}
      className={`ytm-slider${dragged === null ? '' : ' dragging'} ${className}`}
      role="slider"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(shown * 100)}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId)
        const value = at(event)
        setDragged(value)
        onMove?.(value)
      }}
      onPointerMove={(event) => {
        if (dragged === null) return
        const value = at(event)
        setDragged(value)
        onMove?.(value)
      }}
      onPointerUp={(event) => {
        if (dragged === null) return
        onDone(at(event))
        setDragged(null)
      }}
      onPointerCancel={() => setDragged(null)}
    >
      <span className="ytm-slider-track" />
      <span className="ytm-slider-fill" style={{ transform: `scaleX(${shown})` }} />
      <span className="ytm-slider-thumb" style={{ left: `${shown * 100}%` }} />
    </div>
  )
}

/** How far the song is, to drag to another place in it; its time held, breathing, while it waits for its
 * sound. */
function Progress({ playback }: { playback: Playback }) {
  const position = usePosition(playback)
  const [dragged, setDragged] = useState<number | null>(null)
  const duration = playback.duration
  const share = duration > 0 ? position / duration : 0
  return (
    <div className="ytm-progress">
      <Slider
        share={share}
        label={clock(position)}
        className={playback.loading ? 'loading' : ''}
        onMove={(value) => setDragged(value * duration)}
        onDone={(value) => {
          setDragged(null)
          if (duration > 0) seek(value * duration)
        }}
      />
      <div className="ytm-times">
        <span>{clock(dragged ?? position)}</span>
        <span>{duration > 0 ? clock(duration) : ''}</span>
      </div>
    </div>
  )
}

/** The volume (the player's own), quiet to loud. */
function Volume({ playback }: { playback: Playback }) {
  const text = useText()
  return (
    <div className="ytm-volume">
      <VolumeGlyph size={22} />
      <Slider
        share={playback.volume ?? 1}
        label={text.youtubeMusic.volume}
        onMove={(value) => setVolume(value)}
        onDone={(value) => setVolume(value, true)}
      />
      <VolumeGlyph size={22} loud />
    </div>
  )
}

/**
 * Now Playing, as YouTube Music's iPad app has it, laid side by side for the cab's wide screen: on a colour of
 * the cover's, the chevron, the Song/Video switch and ⋮ on top; the cover (or its video's picture) beside the
 * song's title and artist, its likes and Radio, how far it is, the controls and the volume; at the foot Up
 * next, Lyrics and Related, each rising as a sheet over it.
 */
export function NowPlaying() {
  const text = useText()
  const open = useYouTubeMusic((state) => state.nowPlaying)
  const playback = useYouTubeMusic((state) => state.playback)
  const panel = useYouTubeMusic((state) => state.panel)
  const color = useCoverColor(playback?.track.image)
  if (!open || !playback) return null
  const words = text.youtubeMusic
  const track = playback.track
  // The Video side, on a song that has a music video: its picture in the cover's place.
  const picture = playback.video && track.hasVideo
  const close = () => useYouTubeMusic.setState({ nowPlaying: false, panel: null })
  const tabs: Panel[] = ['upNext', 'lyrics', 'related']
  return (
    <div
      className="ytm-now"
      role="dialog"
      aria-label={words.nowPlaying}
      style={color ? ({ '--now-color': color } as CSSProperties) : undefined}
    >
      <header className="ytm-now-top">
        <button type="button" className="ytm-round" aria-label={words.close} onClick={close}>
          <ChevronDownGlyph size={34} />
        </button>
        {track.hasVideo ? (
          <div className="ytm-switch" role="radiogroup">
            <button
              type="button"
              role="radio"
              aria-checked={!playback.video}
              className={playback.video ? '' : 'on'}
              onClick={() => setVideo(false)}
            >
              {words.song}
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={playback.video}
              className={playback.video ? 'on' : ''}
              onClick={() => setVideo(true)}
            >
              {words.video}
            </button>
          </div>
        ) : (
          <span />
        )}
        <button
          type="button"
          className="ytm-round"
          aria-label={words.moreActions}
          onClick={() => useYouTubeMusic.setState({ menu: itemOf(playback) })}
        >
          <MoreGlyph size={28} />
        </button>
      </header>
      <div className="ytm-now-main">
        <div
          key={track.video || track.title}
          className={`ytm-now-art${playback.direction === -1 ? ' back' : ''}${picture ? ' video' : ''}`}
        >
          {picture ? (
            <VideoPicture cover={track.image} />
          ) : (
            <Cover item={{ title: track.title, kind: 'song', image: track.image }} className="ytm-now-cover" />
          )}
          {playback.ad && <span className="ytm-ad">{words.ad}</span>}
        </div>
        <div className="ytm-now-song">
          <div className="ytm-now-text">
            <span className="ytm-now-title">{track.title}</span>
            {track.artist ? (
              <button
                type="button"
                className="ytm-now-artist"
                onClick={() => track.artist && openPage('artist', track.artist, track.artists)}
              >
                {track.artists}
              </button>
            ) : (
              <span className="ytm-now-artist">{track.artists}</span>
            )}
          </div>
          <div className="ytm-now-chips">
            <Likes playback={playback} />
            {track.video && (
              <button type="button" className="ytm-now-chip" onClick={() => startRadio(itemOf(playback))}>
                <RadioGlyph size={22} />
                {words.radio}
              </button>
            )}
          </div>
          <Progress playback={playback} />
          <div className="ytm-controls">
            <button
              type="button"
              className={playback.shuffle ? 'ytm-round on' : 'ytm-round'}
              aria-label={words.shuffle}
              aria-pressed={playback.shuffle}
              onClick={toggleShuffle}
            >
              <ShuffleGlyph size={28} />
            </button>
            <button type="button" className="ytm-round" aria-label={words.previous} onClick={() => skip(-1)}>
              <PreviousGlyph size={38} />
            </button>
            <button
              type="button"
              className="ytm-play"
              aria-label={playback.playing ? words.pause : words.play}
              onClick={playOrPause}
            >
              {playback.playing ? <PauseGlyph size={42} /> : <PlayGlyph size={42} />}
            </button>
            <button type="button" className="ytm-round" aria-label={words.next} onClick={() => skip(1)}>
              <NextGlyph size={38} />
            </button>
            <button
              type="button"
              className={playback.repeat === 'off' ? 'ytm-round' : 'ytm-round on'}
              aria-label={words.repeat}
              aria-pressed={playback.repeat !== 'off'}
              onClick={cycleRepeat}
            >
              <RepeatGlyph size={28} one={playback.repeat === 'one'} />
            </button>
          </div>
          <Volume playback={playback} />
        </div>
      </div>
      <nav className="ytm-now-tabs">
        {tabs.map((each) => (
          <button key={each} type="button" onClick={() => useYouTubeMusic.setState({ panel: each })}>
            {words[each]}
          </button>
        ))}
      </nav>
      {panel && (
        <div className="ytm-sheet" role="dialog" aria-label={words[panel]}>
          <button
            type="button"
            className="ytm-sheet-handle"
            aria-label={words.close}
            onClick={() => useYouTubeMusic.setState({ panel: null })}
          >
            <span />
          </button>
          <nav className="ytm-sheet-tabs">
            {tabs.map((each) => (
              <button
                key={each}
                type="button"
                className={each === panel ? 'on' : ''}
                onClick={() => useYouTubeMusic.setState({ panel: each })}
              >
                {words[each]}
              </button>
            ))}
          </nav>
          <div className="ytm-sheet-body">
            {panel === 'upNext' && <UpNext playback={playback} />}
            {panel === 'lyrics' && <Lyrics video={track.video} />}
            {panel === 'related' && <Related video={track.video} />}
          </div>
        </div>
      )}
    </div>
  )
}

/** Like and dislike, in one capsule (YouTube Music's own). */
function Likes({ playback }: { playback: Playback }) {
  const text = useText()
  const rating = useRating(playback.track)
  if (!playback.track.video) return null
  return (
    <div className="ytm-likes">
      <button
        type="button"
        className={rating === 'like' ? 'on' : ''}
        aria-label={text.youtubeMusic.like}
        aria-pressed={rating === 'like'}
        onClick={() => rate(playback.track, 'like', rating)}
      >
        <ThumbUpGlyph size={24} filled={rating === 'like'} />
      </button>
      <button
        type="button"
        className={rating === 'dislike' ? 'on' : ''}
        aria-label={text.youtubeMusic.dislike}
        aria-pressed={rating === 'dislike'}
        onClick={() => rate(playback.track, 'dislike', rating)}
      >
        <ThumbDownGlyph size={24} filled={rating === 'dislike'} />
      </button>
    </div>
  )
}

/** The queue: what it plays from, then its songs, the one playing marked; one pressed plays. */
function UpNext({ playback }: { playback: Playback }) {
  const text = useText()
  return (
    <div className="ytm-scroll">
      {playback.from && (
        <p className="ytm-from">
          <span>{text.youtubeMusic.playingFrom}</span>
          <b>{playback.from}</b>
        </p>
      )}
      <div className="ytm-songs">
        {playback.queue.map((item, at) => {
          const index = playback.start + at
          const now = index === playback.index
          return (
            <div
              key={`${item.video}-${index}`}
              role="button"
              tabIndex={0}
              className={now ? 'ytm-row now current' : 'ytm-row'}
              onClick={() => !now && jumpTo(index)}
            >
              <span className="ytm-row-cover">
                <Cover item={item} />
                {now && (
                  <span className={`ytm-row-playing ${playback.playing ? 'playing' : 'paused'}`}>
                    <EqualizerGlyph size={24} />
                  </span>
                )}
              </span>
              <span className="ytm-row-text">
                <span className="ytm-row-title">{item.title}</span>
                <span className="ytm-row-sub">{item.subtitle}</span>
              </span>
              {item.duration && <span className="ytm-row-length">{item.duration}</span>}
            </div>
          )
        })}
      </div>
    </div>
  )
}

/** The song's words, and where they're from; or that there are none. */
function Lyrics({ video }: { video: string }) {
  const text = useText()
  const asked = useAsked(video ? { ask: 'lyrics', video } : null)
  if (asked.failed) return <Failed onRetry={asked.retry} />
  if (!asked.answer) return <div className="ytm-lyrics-wait" role="progressbar" />
  return (
    <div className="ytm-scroll">
      {asked.answer.text ? (
        <div className="ytm-lyrics">
          <p>{asked.answer.text}</p>
          {asked.answer.source && <small>{asked.answer.source}</small>}
        </div>
      ) : (
        <p className="ytm-note">{text.youtubeMusic.noLyrics}</p>
      )}
    </div>
  )
}

/** What goes with the song: more like it, playlists, artists like its, about its artist. */
function Related({ video }: { video: string }) {
  const asked = useAsked(video ? { ask: 'related', video } : null)
  if (asked.failed) return <Failed onRetry={asked.retry} />
  if (!asked.answer) return <div className="ytm-lyrics-wait" role="progressbar" />
  return (
    <div className="ytm-scroll">
      {asked.answer.sections.map((section, index) => (
        <Section key={`${section.title}-${index}`} section={section} />
      ))}
    </div>
  )
}

/** A song's ⋮: its choices in a sheet rising over the app (YouTube Music's own): its radio, playing it next or
 * at the queue's end, its album, its artist. */
export function SongMenu() {
  const text = useText()
  const item = useYouTubeMusic((state) => state.menu)
  if (!item) return null
  const words = text.youtubeMusic
  const close = () => useYouTubeMusic.setState({ menu: null })
  const choose = (action: () => void) => {
    close()
    action()
  }
  const queued = { next: words.willPlayNext, end: words.addedToQueue }
  return (
    <div className="ytm-menu-sheet-veil" onClick={close}>
      <div className="ytm-menu-sheet" role="menu" onClick={(event) => event.stopPropagation()}>
        <header className="ytm-menu-sheet-head">
          <Cover item={item} className="ytm-menu-sheet-cover" />
          <span className="ytm-row-text">
            <span className="ytm-row-title">{item.title}</span>
            <span className="ytm-row-sub">{item.subtitle}</span>
          </span>
        </header>
        <button type="button" role="menuitem" onClick={() => choose(() => startRadio(item))}>
          <RadioGlyph size={26} />
          {words.startRadio}
        </button>
        <button type="button" role="menuitem" onClick={() => choose(() => queueSong(item, true, queued))}>
          <PlayNextGlyph size={26} />
          {words.playNext}
        </button>
        <button type="button" role="menuitem" onClick={() => choose(() => queueSong(item, false, queued))}>
          <QueueGlyph size={26} />
          {words.addToQueue}
        </button>
        {item.album && (
          <button
            type="button"
            role="menuitem"
            onClick={() => choose(() => item.album && openPage('album', item.album, ''))}
          >
            <AlbumGlyph size={26} />
            {words.goToAlbum}
          </button>
        )}
        {item.artist && (
          <button
            type="button"
            role="menuitem"
            onClick={() => choose(() => item.artist && openPage('artist', item.artist, ''))}
          >
            <ArtistGlyph size={26} />
            {words.goToArtist}
          </button>
        )}
      </div>
    </div>
  )
}
