import { type CSSProperties, type PointerEvent, useRef, useState } from 'react'
import { useText } from '../../state/text'
import {
  AddGlyph,
  DownGlyph,
  LyricsGlyph,
  NextGlyph,
  PauseGlyph,
  PlayGlyph,
  PreviousGlyph,
  QueueGlyph,
  RepeatGlyph,
  VolumeGlyph,
  ShuffleGlyph,
} from './glyphs'
import { clock, trackCover } from './look'
import { Cover } from './parts'
import {
  currentTrack,
  cycleRepeat,
  jumpTo,
  openView,
  type Playback,
  playOrPause,
  positionOf,
  seek,
  setVolume,
  skip,
  type SpotifyTrack,
  toggleSaved,
  toggleShuffle,
  useAsked,
  useNow,
  useSaved,
  useSpotify,
} from './spotify'
import { LyricsLines } from '../Lyrics'

/** Adds the song to Liked Songs, or takes it out: the plus in a ring, or the green check. */
function LikeButton({ track, className }: { track: SpotifyTrack; className: string }) {
  const text = useText()
  const liked = useSaved(track.uri, track.saved)
  return (
    <button
      type="button"
      className={liked ? `${className} sp-add added` : `${className} sp-add`}
      aria-label={text.spotify.addToLiked}
      aria-pressed={liked}
      onClick={(event) => {
        event.stopPropagation()
        toggleSaved(track.uri, liked, { added: text.spotify.added, removed: text.spotify.removed })
      }}
    >
      <AddGlyph added={liked} />
    </button>
  )
}

/**
 * The mini player, at the foot of the app while Now Playing is closed: on the song's colour, its cover and
 * names, Liked, Play or Pause, and its time as a thin line along the bottom. Pressed, Now Playing opens.
 */
export function MiniPlayer({ aside }: { aside: boolean }) {
  const text = useText()
  const playback = useSpotify((state) => state.playback)
  const open = useSpotify((state) => state.nowPlaying)
  const now = useNow(playback?.playing ?? false)
  const track = currentTrack(playback)
  if (!playback || !track) return null
  const share = track.duration > 0 ? positionOf(playback, now) / track.duration : 0
  return (
    <div
      className={open || aside ? 'sp-mini away' : 'sp-mini'}
      style={{ '--song': track.color } as CSSProperties}
      role="button"
      tabIndex={0}
      aria-label={text.spotify.nowPlaying}
      onClick={() => useSpotify.setState({ nowPlaying: true })}
    >
      <Cover key={track.uri} item={trackCover(track)} className="sp-mini-cover" />
      <span className="sp-mini-text">
        <span className="sp-mini-name">{track.name}</span>
        <span className="sp-mini-artists">{track.artists.map((a) => a.name).join(', ')}</span>
      </span>
      <LikeButton track={track} className="sp-mini-button" />
      <button
        type="button"
        className="sp-mini-button sp-mini-play"
        aria-label={playback.playing ? text.pause : text.play}
        onClick={(event) => {
          event.stopPropagation()
          playOrPause()
        }}
      >
        {playback.playing ? <PauseGlyph /> : <PlayGlyph />}
      </button>
      <span className="sp-mini-time" style={{ '--share': share } as CSSProperties} />
    </div>
  )
}

/**
 * Now Playing, over the app (it rises from the mini player): washed in the song's colour, the cover large
 * (or, the queue open, the queue in its place), the names and Liked, the time, and Spotify's controls with
 * shuffle and repeat green (a dot under) while on.
 */
export function NowPlaying() {
  const text = useText()
  const open = useSpotify((state) => state.nowPlaying)
  const showQueue = useSpotify((state) => state.queue)
  const showLyrics = useSpotify((state) => state.lyrics)
  const playback = useSpotify((state) => state.playback)
  const track = currentTrack(playback)
  if (!playback || !track) return null
  const close = () => useSpotify.setState({ nowPlaying: false })
  const toArtist = (uri: string) => {
    close()
    openView({ kind: 'artist', uri })
  }
  return (
    <div
      className={open ? 'sp-now open' : 'sp-now'}
      style={{ '--song': track.color } as CSSProperties}
      aria-hidden={!open}
    >
      <div className="sp-now-head">
        <button type="button" className="sp-icon-button" aria-label={text.spotify.close} onClick={close}>
          <DownGlyph />
        </button>
        {/* Where it plays from (not known when it was started elsewhere: nothing then). */}
        <div className="sp-now-from">
          {playback.context && (
            <>
              <span>{text.spotify.playingFrom[playback.context.kind]}</span>
              <b>{playback.context.name}</b>
            </>
          )}
        </div>
        <div className="sp-now-tools">
          <button
            type="button"
            className={showLyrics ? 'sp-icon-button sp-toggle on' : 'sp-icon-button sp-toggle'}
            aria-label={text.spotify.lyrics}
            aria-pressed={showLyrics}
            onClick={() => useSpotify.setState({ lyrics: !showLyrics, queue: false })}
          >
            <LyricsGlyph />
          </button>
          <button
            type="button"
            className={showQueue ? 'sp-icon-button sp-toggle on' : 'sp-icon-button sp-toggle'}
            aria-label={text.spotify.queue}
            aria-pressed={showQueue}
            onClick={() => useSpotify.setState({ queue: !showQueue, lyrics: false })}
          >
            <QueueGlyph />
          </button>
        </div>
      </div>
      <div className="sp-now-body">
        <div className="sp-now-art">
          {showQueue ? (
            <Queue playback={playback} />
          ) : showLyrics ? (
            <Words key={track.uri} playback={playback} track={track} />
          ) : (
            <Cover
              key={track.uri}
              item={trackCover(track)}
              className={playback.direction === 1 ? 'sp-now-cover from-right' : 'sp-now-cover from-left'}
            />
          )}
        </div>
        <div className="sp-now-side">
          <div className="sp-now-names">
            <div className="sp-now-text">
              <span key={track.uri} className="sp-now-name">
                {track.name}
              </span>
              <span className="sp-now-artists">
                {track.artists.map((artist, i) => (
                  <span key={artist.uri}>
                    {i > 0 && ', '}
                    <button type="button" onClick={() => toArtist(artist.uri)}>
                      {artist.name}
                    </button>
                  </span>
                ))}
              </span>
            </div>
            <LikeButton track={track} className="sp-now-like" />
          </div>
          <Progress playback={playback} track={track} />
          <div className="sp-controls">
            <button
              type="button"
              className={playback.shuffle ? 'sp-control sp-toggle on' : 'sp-control sp-toggle'}
              aria-label={text.spotify.shuffle}
              aria-pressed={playback.shuffle}
              onClick={toggleShuffle}
            >
              <ShuffleGlyph />
            </button>
            <button
              type="button"
              className="sp-control big"
              aria-label={text.spotify.previous}
              onClick={() => skip(-1)}
            >
              <PreviousGlyph />
            </button>
            <button
              type="button"
              className="sp-control sp-now-play"
              aria-label={playback.playing ? text.pause : text.play}
              onClick={() => playOrPause()}
            >
              {playback.playing ? <PauseGlyph /> : <PlayGlyph />}
            </button>
            <button type="button" className="sp-control big" aria-label={text.spotify.next} onClick={() => skip(1)}>
              <NextGlyph />
            </button>
            <button
              type="button"
              className={playback.repeat !== 'off' ? 'sp-control sp-toggle on' : 'sp-control sp-toggle'}
              aria-label={text.spotify.repeat}
              aria-pressed={playback.repeat !== 'off'}
              onClick={cycleRepeat}
            >
              <RepeatGlyph one={playback.repeat === 'one'} />
            </button>
          </div>
          {playback.volume !== null && <Volume volume={playback.volume} />}
        </div>
      </div>
    </div>
  )
}

/** The playing device's volume: Spotify's speaker (pressed, it mutes, or plays again as loud as it was), and
 * its bar (dragged, the volume follows). */
function Volume({ volume }: { volume: number }) {
  const text = useText()
  const bar = useRef<HTMLDivElement>(null)
  const [dragging, setDragging] = useState(false)
  const before = useRef(volume || 0.5)
  const at = (event: PointerEvent) => {
    const box = bar.current!.getBoundingClientRect()
    return Math.min(1, Math.max(0, (event.clientX - box.left) / box.width))
  }
  return (
    <div className="sp-volume">
      <button
        type="button"
        className="sp-icon-button"
        aria-label={volume === 0 ? text.spotify.unmute : text.spotify.mute}
        onClick={() => {
          if (volume > 0) before.current = volume
          setVolume(volume > 0 ? 0 : before.current, true)
        }}
      >
        <VolumeGlyph volume={volume} />
      </button>
      <div
        ref={bar}
        className={dragging ? 'sp-bar dragging' : 'sp-bar'}
        role="slider"
        aria-label={text.spotify.volume}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(volume * 100)}
        style={{ '--share': volume } as CSSProperties}
        onPointerDown={(event) => {
          event.currentTarget.setPointerCapture(event.pointerId)
          setDragging(true)
          setVolume(at(event))
        }}
        onPointerMove={(event) => {
          if (dragging) setVolume(at(event))
        }}
        onPointerUp={(event) => {
          if (!dragging) return
          setVolume(at(event), true)
          setDragging(false)
        }}
        onPointerCancel={() => setDragging(false)}
      >
        <span className="sp-bar-fill" />
        <span className="sp-bar-knob" />
      </div>
    </div>
  )
}

/** The song's time: the bar (dragged, it seeks; let go, it plays from there) and the times under it. */
function Progress({ playback, track }: { playback: Playback; track: SpotifyTrack }) {
  const now = useNow(playback.playing)
  const bar = useRef<HTMLDivElement>(null)
  const [dragged, setDragged] = useState<number | null>(null)
  const position = dragged ?? positionOf(playback, now)
  const share = track.duration > 0 ? position / track.duration : 0
  const at = (event: PointerEvent) => {
    const box = bar.current!.getBoundingClientRect()
    return Math.min(1, Math.max(0, (event.clientX - box.left) / box.width)) * track.duration
  }
  return (
    <div className="sp-progress">
      <div
        ref={bar}
        className={dragged === null ? 'sp-bar' : 'sp-bar dragging'}
        role="slider"
        aria-valuemin={0}
        aria-valuemax={track.duration}
        aria-valuenow={Math.round(position)}
        style={{ '--share': share } as CSSProperties}
        onPointerDown={(event) => {
          event.currentTarget.setPointerCapture(event.pointerId)
          setDragged(at(event))
        }}
        onPointerMove={(event) => {
          if (dragged !== null) setDragged(at(event))
        }}
        onPointerUp={(event) => {
          if (dragged === null) return
          seek(at(event))
          setDragged(null)
        }}
        onPointerCancel={() => setDragged(null)}
      >
        <span className="sp-bar-fill" />
        <span className="sp-bar-knob" />
      </div>
      <div className="sp-times">
        <span>{clock(position)}</span>
        <span>{clock(track.duration)}</span>
      </div>
    </div>
  )
}

/** The song's words, in the cover's place (Spotify's Lyrics card): in Spotify's colours for the song, the words
 * to come in one and those sung in the other, a line pressed played from; where they're from under. */
function Words({ playback, track }: { playback: Playback; track: SpotifyTrack }) {
  const text = useText()
  const asked = useAsked({ ask: 'lyrics', uri: track.uri })
  const now = useNow(playback.playing)
  const lyrics = asked.answer
  const colors = lyrics?.colors
  const style = colors
    ? ({
        '--lyrics-back': colors.background,
        '--lyrics-text': colors.text,
        '--lyrics-lit': colors.highlight,
      } as CSSProperties)
    : undefined
  return (
    <div className="sp-lyrics-card" style={style}>
      {asked.failed ? (
        <div className="sp-lyrics-note">
          <p>{text.spotify.failed}</p>
          <button type="button" className="sp-pill white small" onClick={asked.retry}>
            {text.spotify.tryAgain}
          </button>
        </div>
      ) : !lyrics ? (
        <div className="sp-lyrics-wait" role="progressbar" />
      ) : lyrics.lines.length === 0 ? (
        <div className="sp-lyrics-note">
          <p>{text.spotify.noLyrics}</p>
        </div>
      ) : (
        <LyricsLines
          className="sp-lyrics"
          lyrics={lyrics}
          position={positionOf(playback, now)}
          onSeek={seek}
          before={!lyrics.synced && <p className="sp-lyrics-unsynced">{text.spotify.notSynced}</p>}
          after={lyrics.provider ? <small>{text.spotify.lyricsFrom(lyrics.provider)}</small> : null}
        />
      )}
    </div>
  )
}

/** The queue: the song now, then the songs to come from where it plays. Pressed, one plays. */
function Queue({ playback }: { playback: Playback }) {
  const text = useText()
  const track = currentTrack(playback)!
  const next = playback.tracks.slice(playback.index + 1, playback.index + 12)
  const jump = (offset: number) => jumpTo(offset)
  return (
    <div className="sp-queue">
      <h3>{text.spotify.nowPlaying}</h3>
      <QueueRow track={track} current />
      {next.length > 0 && (
        <h3>{playback.context?.name ? text.spotify.nextFrom(playback.context.name) : text.spotify.nextUp}</h3>
      )}
      {next.map((song, i) => (
        <QueueRow key={`${song.uri}:${i}`} track={song} onPlay={() => jump(i + 1)} />
      ))}
    </div>
  )
}

function QueueRow({ track, current = false, onPlay }: { track: SpotifyTrack; current?: boolean; onPlay?(): void }) {
  return (
    <button
      type="button"
      className={current ? 'sp-queue-row sp-current' : 'sp-queue-row'}
      onClick={onPlay}
      disabled={current}
    >
      <Cover item={trackCover(track)} />
      <span className="sp-track-text">
        <span className="sp-track-name">{track.name}</span>
        <span className="sp-track-artists">{track.artists.map((a) => a.name).join(', ')}</span>
      </span>
    </button>
  )
}
