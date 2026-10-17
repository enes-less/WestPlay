import { useCallback, useEffect, useRef, useState } from 'react'
import {
  isInHost,
  type YouTubeAnswer,
  type YouTubeChannel,
  type YouTubePlaylist,
  type YouTubeVideo,
} from '../../../shared/host'
import { SearchGlyph } from '../../keyboard/glyphs'
import { useKeyboard } from '../../keyboard/store'
import { TextField } from '../../keyboard/TextField'
import { useLive } from '../../state/store'
import { languageOf } from '../../state/strings'
import { useText } from '../../state/text'
import {
  BackGlyph,
  CloseGlyph,
  FillGlyph,
  HistoryGlyph,
  PauseGlyph,
  PersonGlyph,
  PlayGlyph,
  QueueGlyph,
} from './glyphs'
import {
  ask,
  clearSearches,
  clearWatched,
  command,
  forgetSearch,
  goBack,
  type KnownPlaylist,
  openChannel,
  openPlaylist,
  openResults,
  play,
  type Question,
  type SearchFilter,
  rememberSearch,
  reopenPlayer,
  setLanguage,
  signIn,
  signOut,
  stopVideo,
  useYouTube,
  type View,
  type Watched,
} from './youtube'
import { VideoHole } from './VideoHole'
import './youtube.css'

const FIELD = 'youtube-search'

/**
 * The YouTube app (its player is the full-screen Player, over everything): YouTube's look, sized for the
 * cab. The search pill on top, and the player's picture (signed in) or Sign in beside it; under it Home
 * (signed in, YouTube's own Home and Subscriptions; otherwise what this PC has watched and searched),
 * YouTube's suggestions while typing, a search's results, or a channel's page (pages over Home, Back going
 * back through them). A video opens full screen.
 */
export function YouTubeApp() {
  const text = useText()
  const views = useYouTube((store) => store.views)
  const [query, setQuery] = useState('')
  const typing = useKeyboard((state) => state.field?.id === FIELD)
  const language = languageOf(useLive((state) => state.gameLanguage))
  useEffect(() => setLanguage(language), [language])
  // Left while watching: back to the video.
  useEffect(() => {
    if (useYouTube.getState().returnToPlayer) reopenPlayer(true)
  }, [])

  const search = (wanted: string) => {
    const trimmed = wanted.trim()
    if (!trimmed) return
    rememberSearch(trimmed)
    setQuery(trimmed)
    useKeyboard.getState().detach(FIELD)
    openResults(trimmed)
  }
  const back = () => {
    useKeyboard.getState().detach(FIELD)
    goBack()
  }
  // The field shows the search of the page showing (none on Home or a channel's page).
  const top = views.at(-1)
  const shownQuery = top?.kind === 'results' ? top.query : ''
  const [lastShown, setLastShown] = useState(shownQuery)
  if (shownQuery !== lastShown) {
    // Adjusting state during render: the field follows the page showing.
    setLastShown(shownQuery)
    setQuery(shownQuery)
  }

  return (
    <section className="app-panel youtube-app" aria-label={text.apps.youtube}>
      <div className="yt-top">
        {views.length > 0 && !typing && (
          <button type="button" className="yt-round" aria-label={text.youtube.back} onClick={back}>
            <BackGlyph />
          </button>
        )}
        <TextField
          id={FIELD}
          value={query}
          onChange={setQuery}
          onSubmit={() => search(query)}
          returnKind="search"
          hint={text.youtube.search}
          label={text.youtube.search}
          icon={<SearchGlyph />}
        />
        {!typing && <Account />}
      </div>
      {!typing && <SigningIn />}
      {/* Layers, each kept as it is under the one over it (closing the search, or going back, finds the
          page under as it was, its list and its scroll): Home, the pages over it, suggestions. */}
      <div className="yt-body">
        <div className={views.length === 0 && !typing ? 'yt-layer' : 'yt-layer away'}>
          <Home onSearch={search} />
        </div>
        {views.map((view, index) => (
          <div key={view.id} className={index === views.length - 1 && !typing ? 'yt-layer' : 'yt-layer away'}>
            <Page view={view} />
          </div>
        ))}
        {typing && (
          <div className="yt-layer">
            <Suggestions query={query} onSearch={search} onFill={setQuery} />
          </div>
        )}
      </div>
      {!typing && <NowPlaying />}
    </section>
  )
}

/** A page over Home. */
function Page({ view }: { view: View }) {
  switch (view.kind) {
    case 'results':
      return <Results query={view.query} filter={view.filter} />
    case 'channel':
      return <ChannelPage channel={view.channel} />
    case 'playlist':
      return <PlaylistPage playlist={view.playlist} />
  }
}

/** The video playing while its player is closed (it plays on, heard), its picture moving in small: pressed,
 * it opens full screen again. */
function NowPlaying() {
  const text = useText()
  const current = useYouTube((store) => store.current)
  const open = useYouTube((store) => store.open)
  const state = useYouTube((store) => store.state)
  if (!current || open) return null
  const playing = state?.video === current.video && state.state === 1
  // The video itself once YouTube's player has it (in WestPlay), its thumbnail until then.
  const live = isInHost() && state?.video === current.video
  return (
    <div className="yt-now">
      <button type="button" className="yt-now-open" aria-label={text.youtube.nowPlaying} onClick={() => reopenPlayer()}>
        {live ? <VideoHole className="yt-thumb yt-now-video" /> : <Thumbnail video={current} />}
        <span className="yt-now-text">
          <span className="yt-title">{current.title}</span>
          <span className="yt-sub">{current.channel}</span>
        </span>
      </button>
      <button
        type="button"
        className="yt-round"
        aria-label={playing ? text.pause : text.play}
        onClick={() => command({ action: playing ? 'pause' : 'play' })}
      >
        {playing ? <PauseGlyph /> : <PlayGlyph />}
      </button>
      <button type="button" className="yt-round" aria-label={text.close} onClick={stopVideo}>
        <CloseGlyph />
      </button>
    </div>
  )
}

/** The player's picture, its menu holding Sign out; or, not signed in, Sign in. */
function Account() {
  const text = useText()
  const account = useYouTube((store) => store.account)
  const [menu, setMenu] = useState(false)
  if (!account) return null
  if (!account.signedIn) {
    return (
      <button type="button" className="yt-sign-in" onClick={signIn}>
        <PersonGlyph />
        {text.youtube.signIn}
      </button>
    )
  }
  return (
    <div className="yt-account">
      <button type="button" className="yt-me" aria-label={text.youtube.signOut} onClick={() => setMenu(!menu)}>
        {account.avatar ? <img src={account.avatar} alt="" /> : <PersonGlyph />}
      </button>
      {menu && (
        <div className="yt-menu" role="menu">
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setMenu(false)
              signOut()
            }}
          >
            {text.youtube.signOut}
          </button>
        </div>
      )}
    </div>
  )
}

/** While Google's sign-in is open on the PC: where to finish it. */
function SigningIn() {
  const text = useText()
  const signingIn = useYouTube((store) => store.signingIn)
  return signingIn ? (
    <p className="yt-signing-in" role="status">
      <PersonGlyph />
      {text.youtube.finishOnPc}
    </p>
  ) : null
}

/** Home: signed in, YouTube's own; otherwise this PC's. */
function Home({ onSearch }: { onSearch(query: string): void }) {
  const signedIn = useYouTube((store) => store.account?.signedIn ?? false)
  return signedIn ? <SignedInHome onSearch={onSearch} /> : <LocalHome onSearch={onSearch} />
}

/** A signed-in player's Home: YouTube's recommendations, or (a chip away) their Subscriptions, as cards two
 * across; the other chips search. */
function SignedInHome({ onSearch }: { onSearch(query: string): void }) {
  const text = useText()
  const [feed, setFeed] = useState<'home' | 'subscriptions'>('home')
  return (
    <>
      <div className="yt-chips">
        {(['home', 'subscriptions'] as const).map((which) => (
          <button
            key={which}
            type="button"
            className={feed === which ? 'yt-chip on' : 'yt-chip'}
            aria-pressed={feed === which}
            onClick={() => setFeed(which)}
          >
            {which === 'home' ? text.youtube.home : text.youtube.subscriptions}
          </button>
        ))}
        {text.youtube.chips.map((chip) => (
          <button key={chip.label} type="button" className="yt-chip" onClick={() => onSearch(chip.query)}>
            {chip.label}
          </button>
        ))}
      </div>
      <VideoList
        key={feed}
        layout="grid"
        question={(more) => (more ? { action: 'feed', which: feed, more } : { action: 'feed', which: feed })}
      />
    </>
  )
}

/** Home not signed in: the chips (each a search), the videos watched here and the searches made here. */
function LocalHome({ onSearch }: { onSearch(query: string): void }) {
  const text = useText()
  const watched = useYouTube((store) => store.watched)
  const searches = useYouTube((store) => store.searches)
  return (
    <div className="yt-scroll yt-home">
      <div className="yt-chips">
        {text.youtube.chips.map((chip) => (
          <button key={chip.label} type="button" className="yt-chip" onClick={() => onSearch(chip.query)}>
            {chip.label}
          </button>
        ))}
      </div>
      {watched.length === 0 && searches.length === 0 && <p className="yt-start">{text.youtube.start}</p>}
      {watched.length > 0 && (
        <section>
          <SectionTitle title={text.youtube.continueWatching} onClear={clearWatched} />
          <div className="yt-shelf">
            {watched.slice(0, 12).map((w) => (
              <ShelfCard key={w.video.video} watched={w} />
            ))}
          </div>
        </section>
      )}
      {searches.length > 0 && (
        <section>
          <SectionTitle title={text.youtube.recentSearches} onClear={clearSearches} />
          <div className="yt-chips wrap">
            {searches.map((s) => (
              <span key={s} className="yt-chip yt-recent">
                <button type="button" className="yt-chip-search" onClick={() => onSearch(s)}>
                  <HistoryGlyph />
                  {s}
                </button>
                <button
                  type="button"
                  className="yt-chip-forget"
                  aria-label={text.youtube.forget}
                  onClick={() => forgetSearch(s)}
                >
                  <CloseGlyph />
                </button>
              </span>
            ))}
          </div>
        </section>
      )}
    </div>
  )
}

/** A Home section's title, and its Clear (it forgets what's kept on this PC). */
function SectionTitle({ title, onClear }: { title: string; onClear(): void }) {
  const text = useText()
  return (
    <div className="yt-section-title">
      <h3 className="yt-section">{title}</h3>
      <button type="button" className="yt-clear" onClick={onClear}>
        {text.youtube.clear}
      </button>
    </div>
  )
}

/** A video watched before, on Home's shelf: how far it got under its thumbnail. */
function ShelfCard({ watched }: { watched: Watched }) {
  const { video, time, duration } = watched
  return (
    <button type="button" className="yt-card" onClick={() => play(video)}>
      <Thumbnail video={video} progress={duration > 0 ? time / duration : 0} />
      <span className="yt-title">{video.title}</span>
      <span className="yt-sub">{video.channel}</span>
    </button>
  )
}

/** YouTube's suggestions as the player types (theirs first: the searches made here that match). */
function Suggestions({
  query,
  onSearch,
  onFill,
}: {
  query: string
  onSearch(q: string): void
  onFill(q: string): void
}) {
  const text = useText()
  const searches = useYouTube((store) => store.searches)
  const [suggested, setSuggested] = useState<{ query: string; list: string[] }>({ query: '', list: [] })
  useEffect(() => {
    const trimmed = query.trim()
    if (!trimmed) return
    let current = true
    // A pause in the typing before asking.
    const timer = setTimeout(() => {
      ask({ action: 'suggest', query }).then((answer) => {
        if (current) setSuggested({ query, list: answer.suggestions ?? [] })
      })
    }, 150)
    return () => {
      current = false
      clearTimeout(timer)
    }
  }, [query])

  const typed = query.trim().toLocaleLowerCase()
  const past = (typed ? searches.filter((s) => s.toLocaleLowerCase().startsWith(typed)) : searches).slice(0, 4)
  const theirs = (typed && suggested.query.trim() ? suggested.list : []).filter(
    (s) => !past.some((p) => p.toLocaleLowerCase() === s.toLocaleLowerCase()),
  )
  const rows = [...past.map((s) => ({ s, past: true })), ...theirs.map((s) => ({ s, past: false }))].slice(0, 7)

  return (
    <div className="yt-scroll yt-suggestions">
      {/* Before anything's typed: the searches made here, and their Clear. */}
      {!typed && past.length > 0 && <SectionTitle title={text.youtube.recentSearches} onClear={clearSearches} />}
      {rows.map(({ s, past: isPast }) => (
        <div key={`${isPast}:${s}`} className="yt-suggestion">
          <button type="button" className="yt-suggestion-search" onClick={() => onSearch(s)}>
            {isPast ? <HistoryGlyph /> : <SearchGlyph />}
            <span>
              {/* What's typed plain, the rest bold, as YouTube shows them. */}
              {s.toLocaleLowerCase().startsWith(typed) ? (
                <>
                  {s.slice(0, typed.length)}
                  <b>{s.slice(typed.length)}</b>
                </>
              ) : (
                <b>{s}</b>
              )}
            </span>
          </button>
          <button
            type="button"
            className="yt-suggestion-fill"
            aria-label={text.youtube.fill}
            onClick={() => onFill(`${s} `)}
          >
            <FillGlyph />
          </button>
        </div>
      ))}
    </div>
  )
}

/** A search's results, a row per video or playlist (the channels found first), or (the chips) channels or
 * playlists only. */
function Results({ query, filter }: { query: string; filter: SearchFilter }) {
  const text = useText()
  const filters: { filter: SearchFilter; label: string }[] = [
    { filter: 'all', label: text.youtube.all },
    { filter: 'channels', label: text.youtube.channels },
    { filter: 'playlists', label: text.youtube.playlists },
  ]
  return (
    <>
      <div className="yt-chips">
        {filters.map((chip) => (
          <button
            key={chip.filter}
            type="button"
            className={filter === chip.filter ? 'yt-chip on' : 'yt-chip'}
            aria-pressed={filter === chip.filter}
            onClick={() => filter !== chip.filter && openResults(query, chip.filter)}
          >
            {chip.label}
          </button>
        ))}
      </div>
      <VideoList
        layout="rows"
        question={(more) => {
          const asked: Question = more ? { action: 'search', query, more } : { action: 'search', query }
          return filter === 'all' ? asked : { ...asked, filter }
        }}
      />
    </>
  )
}

/** A playlist's page: its cover, title, channel and YouTube's counts, and Play all; then its videos, in
 * order. A video pressed plays in it: the playlist goes on from there. */
function PlaylistPage({ playlist: known }: { playlist: KnownPlaylist }) {
  const text = useText()
  const [playlist, setPlaylist] = useState<KnownPlaylist>(known)
  // The playlist's videos as loaded so far, and the token for more: the queue a video plays in.
  const [loaded, setLoaded] = useState<{ videos: YouTubeVideo[]; more: string | null }>({ videos: [], more: null })
  const playFrom = (video: YouTubeVideo) =>
    play(video, true, { playlist: known.playlist, title: playlist.title, videos: loaded.videos, more: loaded.more })
  return (
    <VideoList
      layout="rows"
      header={
        <div className="yt-playlist-head">
          <PlaylistThumbnail playlist={playlist} />
          <div className="yt-playlist-text">
            <h2>{playlist.title}</h2>
            {playlist.channel && (
              <ChannelLink video={{ channel: playlist.channel, channelId: playlist.channelId }} className="yt-channel">
                {playlist.channel}
              </ChannelLink>
            )}
            <span className="yt-sub">{[playlist.count, playlist.views].filter(Boolean).join(' · ')}</span>
            <button
              type="button"
              className="yt-play-all"
              disabled={loaded.videos.length === 0}
              onClick={() => playFrom(loaded.videos[0])}
            >
              <PlayGlyph />
              {text.youtube.playAll}
            </button>
          </div>
        </div>
      }
      onAnswer={(answer) => {
        if (answer.playlist) setPlaylist(answer.playlist)
        setLoaded((before) => {
          const seen = new Set(before.videos.map((v) => v.video))
          const added = (answer.videos ?? []).filter((v) => !seen.has(v.video))
          return { videos: [...before.videos, ...added], more: answer.more ?? null }
        })
      }}
      onPlay={playFrom}
      question={(more) =>
        more ? { action: 'playlist', playlist: known.playlist, more } : { action: 'playlist', playlist: known.playlist }
      }
    />
  )
}

/** A playlist a search found: its cover (a stack), its title, channel and count; pressed, its page. */
function PlaylistRow({ playlist }: { playlist: YouTubePlaylist }) {
  const text = useText()
  return (
    <div role="button" tabIndex={0} className="yt-row" onClick={() => openPlaylist(playlist)}>
      <PlaylistThumbnail playlist={playlist} />
      <span className="yt-row-text">
        <span className="yt-title">{playlist.title}</span>
        <span className="yt-sub">{text.youtube.playlist}</span>
        {playlist.channel && (
          <ChannelLink video={playlist} className="yt-channel">
            {playlist.channel}
          </ChannelLink>
        )}
      </span>
    </div>
  )
}

/** A playlist's cover: its first video's thumbnail on a stack (YouTube's look for a playlist), its count on it. */
function PlaylistThumbnail({ playlist }: { playlist: KnownPlaylist }) {
  return (
    <span className="yt-stack">
      <span className="yt-thumb">
        {playlist.thumbnail ? (
          <img src={playlist.thumbnail} alt="" loading="lazy" />
        ) : (
          <span className="yt-thumb-blank" />
        )}
        {playlist.count && (
          <span className="yt-badge yt-count">
            <QueueGlyph />
            {playlist.count}
          </span>
        )}
      </span>
    </span>
  )
}

/** A channel's page: its banner, picture, name and YouTube's words for it, then its videos, two across. */
function ChannelPage({
  channel: known,
}: {
  channel: View & { kind: 'channel' } extends { channel: infer C } ? C : never
}) {
  const [channel, setChannel] = useState<Partial<YouTubeChannel> & { name: string }>(known)
  return (
    <VideoList
      layout="grid"
      header={
        <div className="yt-channel-head">
          {channel.banner && <img className="yt-banner" src={channel.banner} alt="" />}
          <div className="yt-channel-who">
            <ChannelAvatar channel={channel} large />
            <div className="yt-channel-text">
              <h2>{channel.name}</h2>
              <span className="yt-sub">
                {[channel.handle, channel.subscribers, channel.videos].filter(Boolean).join(' · ')}
              </span>
            </div>
            {typeof channel.subscribed === 'boolean' && (
              <SubscribeButton
                channel={known.channel}
                subscribed={channel.subscribed}
                onChange={(subscribed) => setChannel((before) => ({ ...before, subscribed }))}
              />
            )}
          </div>
        </div>
      }
      onAnswer={(answer) => {
        if (answer.channel) setChannel(answer.channel)
      }}
      question={(more) =>
        more ? { action: 'channel', channel: known.channel, more } : { action: 'channel', channel: known.channel }
      }
    />
  )
}

/** YouTube's Subscribe (white) / Subscribed (grey) pill, for the signed-in player: it changes as it's
 * pressed, and back if YouTube refuses. */
function SubscribeButton({
  channel,
  subscribed,
  onChange,
}: {
  channel: string
  subscribed: boolean
  onChange(subscribed: boolean): void
}) {
  const text = useText()
  const [asking, setAsking] = useState(false)
  return (
    <button
      type="button"
      className={subscribed ? 'yt-subscribe subscribed' : 'yt-subscribe'}
      aria-pressed={subscribed}
      disabled={asking}
      onClick={() => {
        const on = !subscribed
        onChange(on)
        setAsking(true)
        ask({ action: 'subscribe', channel, on }).then((answer) => {
          setAsking(false)
          if (answer.failed) onChange(!on)
        })
      }}
    >
      {subscribed ? text.youtube.subscribed : text.youtube.subscribe}
    </button>
  )
}

/**
 * Videos YouTube lists (a search's, a feed's, a channel's, a playlist's), more asked for as the list nears its
 * end: a row per video, or cards two across; a search's channels on top, its playlists among its videos
 * where YouTube put them. `question` asks for the first videos (`more` null) or the next ones; `header` sits
 * above, scrolling with them; `onAnswer` hears each answer; `onPlay` plays a video pressed (otherwise it
 * plays on its own).
 */
function VideoList({
  question,
  layout,
  header,
  onAnswer,
  onPlay = (video) => play(video),
}: {
  question(more: string | null): Question
  layout: 'rows' | 'grid'
  header?: React.ReactNode
  onAnswer?(answer: YouTubeAnswer): void
  onPlay?(video: YouTubeVideo): void
}) {
  const text = useText()
  // The videos, and the playlists each with where it goes: before the video at `at` (at the end past them).
  const [{ videos, playlists }, setList] = useState<{
    videos: YouTubeVideo[]
    playlists: (YouTubePlaylist & { at: number })[]
  }>({ videos: [], playlists: [] })
  const [channels, setChannels] = useState<YouTubeChannel[]>([])
  const [more, setMore] = useState<string | null>(null)
  const [status, setStatus] = useState<'loading' | 'done' | 'failed'>('loading')
  // The list asks with the question it started with (a new question is a new list: its `key`).
  const asked = useRef({ question, onAnswer })
  const asking = useRef(false)
  const load = useCallback((next: string | null) => {
    if (asking.current) return
    asking.current = true
    setStatus('loading')
    ask(asked.current.question(next)).then((answer: YouTubeAnswer) => {
      asking.current = false
      if (answer.failed) {
        setStatus('failed')
        return
      }
      asked.current.onAnswer?.(answer)
      setList((before) => {
        // YouTube can list a video twice across pages.
        const seen = new Set(before.videos.map((v) => v.video))
        const added = (answer.videos ?? []).filter((v) => !seen.has(v.video))
        // The answer's playlists after the videos already listed, where they sit among its own.
        const found = (answer.playlists ?? [])
          .filter((p) => !before.playlists.some((had) => had.playlist === p.playlist))
          .map((p) => ({ ...p, at: before.videos.length + Math.min(p.after, added.length) }))
        return { videos: [...before.videos, ...added], playlists: [...before.playlists, ...found] }
      })
      if (answer.channels?.length) setChannels((before) => [...before, ...(answer.channels ?? [])])
      setMore(answer.more ?? null)
      setStatus('done')
    })
  }, [])
  useEffect(() => load(null), [load])

  const nearEnd = (element: HTMLDivElement) =>
    element.scrollTop + element.clientHeight > element.scrollHeight - element.clientHeight * 0.8
  return (
    <div
      className={layout === 'grid' ? 'yt-scroll yt-grid' : 'yt-scroll yt-results'}
      onScroll={(event) => {
        if (more && status === 'done' && nearEnd(event.currentTarget)) load(more)
      }}
    >
      {header}
      {channels.map((channel) => (
        <button key={channel.channel} type="button" className="yt-channel-row" onClick={() => openChannel(channel)}>
          <span className="yt-channel-row-picture">
            <ChannelAvatar channel={channel} large />
          </span>
          <span className="yt-row-text">
            <span className="yt-title">{channel.name}</span>
            <span className="yt-sub">{[channel.handle, channel.subscribers].filter(Boolean).join(' · ')}</span>
          </span>
        </button>
      ))}
      {videos.map((video, index) => [
        ...playlists.filter((p) => p.at === index).map((p) => <PlaylistRow key={p.playlist} playlist={p} />),
        layout === 'grid' ? (
          <div
            key={video.video}
            role="button"
            tabIndex={0}
            className="yt-card yt-feed-card"
            onClick={() => onPlay(video)}
          >
            <Thumbnail video={video} />
            <span className="yt-meta">
              <ChannelLink video={video}>
                <Avatar video={video} />
              </ChannelLink>
              <span className="yt-meta-text">
                <span className="yt-title">{video.title}</span>
                <span className="yt-sub">{[video.channel, video.views, video.age].filter(Boolean).join(' · ')}</span>
              </span>
            </span>
          </div>
        ) : (
          <div key={video.video} role="button" tabIndex={0} className="yt-row" onClick={() => onPlay(video)}>
            <Thumbnail video={video} />
            <span className="yt-row-text">
              <span className="yt-title">{video.title}</span>
              <span className="yt-sub">{[video.views, video.age].filter(Boolean).join(' · ')}</span>
              <ChannelLink video={video} className="yt-channel">
                <Avatar video={video} />
                {video.channel}
              </ChannelLink>
            </span>
          </div>
        ),
      ])}
      {playlists
        .filter((p) => p.at >= videos.length)
        .map((p) => (
          <PlaylistRow key={p.playlist} playlist={p} />
        ))}
      {status === 'loading' && <Skeletons layout={layout} count={videos.length > 0 ? 2 : layout === 'grid' ? 6 : 5} />}
      {status === 'done' && videos.length === 0 && channels.length === 0 && playlists.length === 0 && (
        <p className="yt-note">{text.youtube.noResults}</p>
      )}
      {status === 'failed' && (
        <p className="yt-note">
          {text.youtube.failed}{' '}
          <button type="button" className="yt-chip" onClick={() => load(videos.length ? more : null)}>
            {text.youtube.tryAgain}
          </button>
        </p>
      )}
    </div>
  )
}

/** Videos still coming: grey shapes of the cards or rows they'll be, breathing (YouTube's own way to wait). */
function Skeletons({ layout, count }: { layout: 'rows' | 'grid'; count: number }) {
  return (
    <>
      {Array.from({ length: count }, (_, index) =>
        layout === 'grid' ? (
          <div key={index} className="yt-card yt-feed-card yt-skeleton" role="progressbar">
            <span className="yt-thumb" />
            <span className="yt-meta">
              <span className="yt-avatar" />
              <span className="yt-meta-text">
                <span className="yt-skeleton-line" />
                <span className="yt-skeleton-line short" />
              </span>
            </span>
          </div>
        ) : (
          <div key={index} className="yt-row yt-skeleton" role="progressbar">
            <span className="yt-thumb" />
            <span className="yt-row-text">
              <span className="yt-skeleton-line" />
              <span className="yt-skeleton-line short" />
              <span className="yt-channel">
                <span className="yt-avatar" />
                <span className="yt-skeleton-line shorter" />
              </span>
            </span>
          </div>
        ),
      )}
    </>
  )
}

/** A video's channel, pressed: the channel's page (the video's own press is not). Plain without its id. */
function ChannelLink({
  video,
  className,
  children,
}: {
  /** A video's or a playlist's channel: its name, and its id and picture when known. */
  video: { channel: string; channelId?: string; avatar?: string | null }
  className?: string
  children: React.ReactNode
}) {
  if (!video.channelId) return <span className={className}>{children}</span>
  const id = video.channelId
  return (
    <button
      type="button"
      className={className ? `yt-channel-link ${className}` : 'yt-channel-link'}
      onClick={(event) => {
        event.stopPropagation()
        openChannel({ channel: id, name: video.channel, avatar: video.avatar ?? null })
      }}
    >
      {children}
    </button>
  )
}

/** A channel's round picture (its initial without one). */
function ChannelAvatar({
  channel,
  large = false,
}: {
  channel: { name: string; avatar?: string | null }
  large?: boolean
}) {
  const className = large ? 'yt-avatar large' : 'yt-avatar'
  return channel.avatar ? (
    <img className={className} src={channel.avatar} alt="" loading="lazy" />
  ) : (
    <span className={`${className} blank`}>{channel.name.slice(0, 1).toLocaleUpperCase()}</span>
  )
}

/** A video's thumbnail, rounded, with its duration (or LIVE) and how much of it was watched. */
export function Thumbnail({ video, progress = 0 }: { video: YouTubeVideo; progress?: number }) {
  const text = useText()
  return (
    <span className="yt-thumb">
      {video.thumbnail ? <img src={video.thumbnail} alt="" loading="lazy" /> : <span className="yt-thumb-blank" />}
      {video.live ? (
        <span className="yt-badge live">{text.youtube.live}</span>
      ) : (
        video.duration && <span className="yt-badge">{video.duration}</span>
      )}
      {progress > 0 && <span className="yt-progress" style={{ width: `${Math.min(100, progress * 100)}%` }} />}
    </span>
  )
}

/** The channel's round picture (its initial without one). */
function Avatar({ video }: { video: YouTubeVideo }) {
  return video.avatar ? (
    <img className="yt-avatar" src={video.avatar} alt="" loading="lazy" />
  ) : (
    <span className="yt-avatar blank">{video.channel.slice(0, 1).toLocaleUpperCase()}</span>
  )
}
