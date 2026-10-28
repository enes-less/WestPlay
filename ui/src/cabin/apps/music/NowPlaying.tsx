import { type CSSProperties, type PointerEvent, useRef, useState } from 'react'
import { useLook } from '../../state/hooks'
import { useCabin } from '../../state/store'
import { useText } from '../../state/text'
import {
  type AppleMusicTrack,
  currentTrack,
  cycleRepeat,
  jumpTo,
  openAlbum,
  openArtist,
  type Playback,
  playOrPause,
  positionOf,
  seek,
  setVolume,
  signIn,
  skip,
  toggleFavorite,
  toggleShuffle,
  useAppleMusic,
  useAsked,
  useFavorite,
  useNow,
} from './appleMusic'
import { LyricsLines } from '../Lyrics'
import { Crossfade } from './Crossfade'
import {
  ChevronDownGlyph,
  LyricsGlyph,
  NextGlyph,
  NoteGlyph,
  PauseGlyph,
  PlayGlyph,
  PreviousGlyph,
  QueueGlyph,
  RepeatGlyph,
  RepeatOneGlyph,
  ShuffleGlyph,
  SpeakerGlyph,
  StarGlyph,
} from './glyphs'
import { type Backdrop, type Look, useBackdropLight, useWordsOver, wordsClass } from './contrast'
import { clock, trackCover } from './look'
import { Cover, Explicit, Failed, Loading } from './parts'

/**
 * The mini player: a glass capsule at the foot of the app while Now Playing is closed (iOS 26's), the song's
 * cover and names, Play or Pause and Next. Pressed, Now Playing opens.
 */
export function MiniPlayer({ aside }: { aside: boolean }) {
  const text = useText()
  const playback = useAppleMusic((state) => state.playback)
  const open = useAppleMusic((state) => state.nowPlaying)
  const now = useNow(playback?.playing ?? false)
  const track = currentTrack(playback)
  const bar = useRef<HTMLDivElement>(null)
  const away = open || aside
  // Over something light, its words go dark (Apple's glass): smoothly (appleMusic.css).
  const light = useBackdropLight(bar, Boolean(track) && !away)
  if (!playback || !track) return null
  const share = playback.duration > 0 ? positionOf(playback, now) / playback.duration : 0
  return (
    <div
      ref={bar}
      className={`am-mini glass${away ? ' away' : ''}${light ? ' on-light' : ''}`}
      role="button"
      tabIndex={0}
      aria-label={text.appleMusic.nowPlaying}
      onClick={() => useAppleMusic.setState({ nowPlaying: true })}
    >
      <Cover key={track.id} item={trackCover(track)} className="am-mini-cover" />
      <span className="am-mini-text">
        <span className="am-mini-name">{track.name}</span>
        <span className="am-mini-by">
          {playback.preview && <span className="am-badge">{text.appleMusic.preview}</span>}
          {track.artist}
        </span>
      </span>
      <button
        type="button"
        className="am-mini-button"
        aria-label={playback.playing ? text.pause : text.play}
        onClick={(event) => {
          event.stopPropagation()
          playOrPause()
        }}
      >
        {playback.playing ? <PauseGlyph size={36} /> : <PlayGlyph size={36} />}
      </button>
      <button
        type="button"
        className="am-mini-button"
        aria-label={text.appleMusic.next}
        onClick={(event) => {
          event.stopPropagation()
          skip(1)
        }}
      >
        <NextGlyph size={38} />
      </button>
      <span
        className={playback.loading ? 'am-mini-time loading' : 'am-mini-time'}
        style={{ '--share': share } as CSSProperties}
      />
    </div>
  )
}

/** Behind Now Playing's words, in each look: the blurred cover under its black veil or its white (music.css). */
const NOW_PLAYING_BACKDROPS: Record<Look, Backdrop> = {
  dark: { cover: 0.58, rest: [0, 0, 0] },
  light: { cover: 0.55, rest: [255, 255, 255] },
}

/**
 * Now Playing, over the app (it rises from the mini player): CarPlay's own, as the Music app always had it.
 * The cover's colours fill the screen; the song and its controls on the left, the cover on the right (or, Up
 * Next open, the songs to come in its place); the time under them; then shuffle, repeat, Favourite and the
 * volume. A song's preview says so, with Sign In beside it.
 */
export function NowPlaying() {
  const text = useText()
  const open = useAppleMusic((state) => state.nowPlaying)
  const showQueue = useAppleMusic((state) => state.queue)
  const showLyrics = useAppleMusic((state) => state.lyrics)
  const playback = useAppleMusic((state) => state.playback)
  const signingIn = useAppleMusic((state) => state.signingIn)
  const albumArt = useCabin((state) => state.albumArt)
  const track = currentTrack(playback)
  const cover = albumArt && track ? track.image : null
  const ink = useWordsOver(cover, useLook().appearance, NOW_PLAYING_BACKDROPS)
  if (!playback || !track) return null
  const close = () => useAppleMusic.setState({ nowPlaying: false })
  const words = text.appleMusic
  return (
    <div
      className={`am-now ${wordsClass(ink)}${open ? ' open' : ''}`}
      aria-hidden={!open}
      style={{ '--cover': track.color } as CSSProperties}
    >
      <Crossfade value={cover} className="music-backdrop" render={(url) => url && <img src={url} alt="" />} />
      <section className="app-panel music-app am-now-panel" aria-label={words.nowPlaying}>
        <div className="music-layout">
          <div className="music-header">
            <button type="button" className="music-corner glass" aria-label={words.close} onClick={close}>
              <ChevronDownGlyph size={34} />
            </button>
            <div className="am-corners">
              <button
                type="button"
                className={showLyrics ? 'music-corner glass am-on' : 'music-corner glass'}
                aria-label={words.lyrics}
                aria-pressed={showLyrics}
                onClick={() => useAppleMusic.setState({ lyrics: !showLyrics, queue: false })}
              >
                <LyricsGlyph size={32} />
              </button>
              <button
                type="button"
                className={showQueue ? 'music-corner glass am-on' : 'music-corner glass'}
                aria-label={words.upNext}
                aria-pressed={showQueue}
                onClick={() => useAppleMusic.setState({ queue: !showQueue, lyrics: false })}
              >
                <QueueGlyph size={32} />
              </button>
            </div>
          </div>

          <div className="music-info">
            <div
              key={track.id}
              className={playback.direction === 1 ? 'am-now-text from-right' : 'am-now-text from-left'}
            >
              <span className="music-title">
                {track.name}
                {track.explicit && <Explicit />}
              </span>
              {track.artistId ? (
                <button
                  type="button"
                  className="music-artist am-now-link"
                  onClick={() => {
                    close()
                    openArtist(track.artistId!, track.artist)
                  }}
                >
                  {track.artist}
                </button>
              ) : (
                <span className="music-artist">{track.artist}</span>
              )}
              {track.album &&
                (track.albumId ? (
                  <button
                    type="button"
                    className="music-album am-now-link"
                    onClick={() => {
                      close()
                      openAlbum(track)
                    }}
                  >
                    {track.album}
                  </button>
                ) : (
                  <span className="music-album">{track.album}</span>
                ))}
              {playback.preview && (
                <span className="am-now-preview">
                  <span className="am-badge">{words.preview}</span>
                  <button type="button" className="am-now-link" onClick={signIn} disabled={signingIn}>
                    {signingIn ? words.finishOnPc : words.previewBanner.title}
                  </button>
                </span>
              )}
            </div>

            <div className="music-controls">
              <button type="button" className="music-button" aria-label={words.previous} onClick={() => skip(-1)}>
                <PreviousGlyph size={54} />
              </button>
              <button
                type="button"
                className="music-button primary"
                aria-label={playback.playing ? text.pause : text.play}
                onClick={playOrPause}
              >
                {playback.playing ? <PauseGlyph size={72} /> : <PlayGlyph size={72} />}
              </button>
              <button type="button" className="music-button" aria-label={words.next} onClick={() => skip(1)}>
                <NextGlyph size={54} />
              </button>
            </div>
          </div>

          {showQueue ? (
            <UpNext playback={playback} />
          ) : showLyrics ? (
            // Keyed apart from the time bar beside it: siblings sharing a key (the song's id, both) weren't told
            // apart, and each song's words stayed on the page under the next's (owner 2026-10-21).
            <Words key={`words:${track.id}`} playback={playback} track={track} />
          ) : (
            <Crossfade
              value={cover}
              className="music-art"
              render={(url) =>
                url ? (
                  <img src={url} alt="" />
                ) : (
                  <div className="music-art-placeholder am-art-drawn" aria-hidden>
                    <NoteGlyph size={72} />
                  </div>
                )
              }
            />
          )}

          <Progress key={`progress:${track.id}`} playback={playback} />

          <div className="music-extras am-extras">
            <button
              type="button"
              className={playback.shuffle ? 'music-button small active' : 'music-button small'}
              aria-label={text.shuffle}
              aria-pressed={playback.shuffle}
              onClick={toggleShuffle}
            >
              <ShuffleGlyph size={36} />
            </button>
            <button
              type="button"
              className={playback.repeat !== 'off' ? 'music-button small active' : 'music-button small'}
              aria-label={words.repeat}
              aria-pressed={playback.repeat !== 'off'}
              onClick={cycleRepeat}
            >
              {playback.repeat === 'one' ? <RepeatOneGlyph size={36} /> : <RepeatGlyph size={36} />}
            </button>
            <FavoriteButton track={track} />
            {playback.volume !== null && <Volume volume={playback.volume} />}
          </div>
        </div>
      </section>
    </div>
  )
}

/** Favourite: Apple Music's star, filled once the song is one. */
function FavoriteButton({ track }: { track: AppleMusicTrack }) {
  const text = useText()
  const favorite = useFavorite(track)
  return (
    <button
      type="button"
      className={favorite ? 'music-button small active' : 'music-button small'}
      aria-label={text.appleMusic.favorite}
      aria-pressed={favorite}
      onClick={() =>
        toggleFavorite(track, favorite, { on: text.appleMusic.favorited, off: text.appleMusic.unfavorited })
      }
    >
      <StarGlyph size={36} on={favorite} />
    </button>
  )
}

/** A bar to drag (the song's time, the volume): where the pointer is along it, 0 to 1. */
function shareAt(bar: HTMLElement, event: PointerEvent): number {
  const box = bar.getBoundingClientRect()
  return Math.min(1, Math.max(0, (event.clientX - box.left) / box.width))
}

/** The player's volume: the speaker (pressed, it mutes, or plays again as loud as it was), and its bar. */
function Volume({ volume }: { volume: number }) {
  const text = useText()
  const bar = useRef<HTMLDivElement>(null)
  const [dragging, setDragging] = useState(false)
  const before = useRef(volume || 0.5)
  return (
    <div className="am-volume">
      <button
        type="button"
        className="music-button small"
        aria-label={volume === 0 ? text.appleMusic.unmute : text.appleMusic.mute}
        onClick={() => {
          if (volume > 0) before.current = volume
          setVolume(volume > 0 ? 0 : before.current, true)
        }}
      >
        <SpeakerGlyph size={34} volume={volume} />
      </button>
      <div
        ref={bar}
        className={dragging ? 'am-slider dragging' : 'am-slider'}
        role="slider"
        aria-label={text.appleMusic.volume}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(volume * 100)}
        style={{ '--share': volume } as CSSProperties}
        onPointerDown={(event) => {
          event.currentTarget.setPointerCapture(event.pointerId)
          setDragging(true)
          setVolume(shareAt(bar.current!, event))
        }}
        onPointerMove={(event) => {
          if (dragging) setVolume(shareAt(bar.current!, event))
        }}
        onPointerUp={(event) => {
          if (!dragging) return
          setVolume(shareAt(bar.current!, event), true)
          setDragging(false)
        }}
        onPointerCancel={() => setDragging(false)}
      >
        <span className="am-slider-fill" />
      </div>
    </div>
  )
}

/** The song's time: the bar (dragged, it seeks; let go, it plays from there) between the time played and the
 * time left. */
function Progress({ playback }: { playback: Playback }) {
  const now = useNow(playback.playing)
  const bar = useRef<HTMLDivElement>(null)
  const [dragged, setDragged] = useState<number | null>(null)
  const length = playback.duration
  const position = dragged ?? positionOf(playback, now)
  const share = length > 0 ? position / length : 0
  return (
    <div className="music-progress">
      <span className="music-time">{clock(position)}</span>
      <div
        ref={bar}
        className={`am-slider am-time${dragged === null ? '' : ' dragging'}${playback.loading ? ' loading' : ''}`}
        role="slider"
        aria-valuemin={0}
        aria-valuemax={Math.round(length)}
        aria-valuenow={Math.round(position)}
        style={{ '--share': share } as CSSProperties}
        onPointerDown={(event) => {
          if (length <= 0) return
          event.currentTarget.setPointerCapture(event.pointerId)
          setDragged(shareAt(bar.current!, event) * length)
        }}
        onPointerMove={(event) => {
          if (dragged !== null) setDragged(shareAt(bar.current!, event) * length)
        }}
        onPointerUp={(event) => {
          if (dragged === null) return
          seek(shareAt(bar.current!, event) * length)
          setDragged(null)
        }}
        onPointerCancel={() => setDragged(null)}
      >
        <span className="am-slider-fill" />
      </div>
      <span className="music-time">-{clock(Math.max(0, length - position))}</span>
    </div>
  )
}

/** The song's words, in the cover's place (Apple Music's Lyrics): big and bold, the line sung bright, a line
 * pressed played from; who wrote them under. */
function Words({ playback, track }: { playback: Playback; track: AppleMusicTrack }) {
  const text = useText()
  const asked = useAsked({ ask: 'lyrics', id: track.id })
  const now = useNow(playback.playing)
  const lyrics = asked.answer
  if (asked.failed) {
    return (
      <div className="am-lyrics-box">
        <Failed onRetry={asked.retry} />
      </div>
    )
  }
  if (!lyrics) {
    return (
      <div className="am-lyrics-box">
        <Loading />
      </div>
    )
  }
  if (lyrics.lines.length === 0) {
    return (
      <div className="am-lyrics-box">
        <p className="am-lyrics-none">{text.appleMusic.noLyrics}</p>
      </div>
    )
  }
  return (
    <LyricsLines
      className="am-lyrics"
      lyrics={lyrics}
      position={positionOf(playback, now)}
      onSeek={seek}
      after={
        lyrics.writers?.length ? (
          <small className="am-lyrics-credit">{text.appleMusic.writtenBy(lyrics.writers.join(', '))}</small>
        ) : null
      }
    />
  )
}

/** Up Next, in the cover's place: the songs to come (from where it plays). Pressed, one plays. */
function UpNext({ playback }: { playback: Playback }) {
  const text = useText()
  const next = playback.tracks.slice(playback.index + 1, playback.index + 30)
  return (
    <div className="am-up-next">
      <h3>{playback.context?.name ? text.appleMusic.from(playback.context.name) : text.appleMusic.upNext}</h3>
      {next.length === 0 && <p className="am-empty">{text.appleMusic.empty}</p>}
      {next.map((song, i) => (
        <button key={`${song.id}:${i}`} type="button" className="am-song" onClick={() => jumpTo(i + 1)}>
          <span className="am-song-cover">
            <Cover item={trackCover(song)} />
          </span>
          <span className="am-song-text">
            <span className="am-song-name">{song.name}</span>
            <span className="am-song-by">{song.artist}</span>
          </span>
        </button>
      ))}
    </div>
  )
}
