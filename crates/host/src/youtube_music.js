// WestPlay, in YouTube Music's web player (youtube_music.rs): YouTube Music's own page, run hidden under the cab.
// The YouTube Music app the player sees is WestPlay's own; this answers its questions and plays as it asks,
// through the page's own player:
//
// - Questions (Home, Explore, a page, a search, an album's, playlist's or artist's page, the Library, a song's
//   words and what goes with it) are asked of YouTube Music's API as its own site asks them, from this page
//   (its identity, and its sign-in when there is one).
// - Playing is the page's own: a play is its own move to the song (as a click on it is), and the controls are
//   its player bar's own (next, previous, shuffle, repeat), so its queue, radio and autoplay are YouTube
//   Music's as ever. What plays is said as the page has it.
// - Signing in is Google's own page, in a window on the desktop; the page then loads again, signed in.
//
// Nothing of the account leaves the page: its session cookie only signs its requests (as YouTube Music's own
// site signs them), to YouTube. Of the account, only whether it's signed in and its picture (for the account's
// button), never its name.
;(() => {
  if (location.hostname !== 'music.youtube.com' || window.top !== window) return
  const post = (message) => window.chrome?.webview?.postMessage(message)
  const stringify = JSON.stringify
  const parse = JSON.parse

  // --- No ads -------------------------------------------------------------------------------------------------
  //
  // YouTube Music's player data says where ads go; taken out wherever it comes from (as uBlock Origin does),
  // the player plays none.
  const AD_KEYS = ['adPlacements', 'adSlots', 'playerAds', 'adBreakHeartbeatParams']
  const strip = (data) => {
    if (data && typeof data === 'object') {
      for (const key of AD_KEYS) delete data[key]
      if (data.playerResponse) strip(data.playerResponse)
    }
    return data
  }
  JSON.parse = function (...args) {
    return strip(parse.apply(this, args))
  }
  const json = Response.prototype.json
  Response.prototype.json = async function () {
    return strip(await json.call(this))
  }
  let initial
  Object.defineProperty(window, 'ytInitialPlayerResponse', {
    configurable: true,
    get: () => initial,
    set: (value) => (initial = strip(value)),
  })

  // --- Past YouTube's warning -------------------------------------------------------------------------------------
  //
  // A song YouTube shows a warning before ("this video may contain disturbing images: I understand and want to
  // continue") stood still at 0 s behind it, nobody there to press continue (a player's, 2026-10-27; YouTube's
  // own was fixed so, youtube.js). The player's request for a song says continue, as that button's does.
  JSON.stringify = function (value, ...rest) {
    try {
      if (typeof value?.videoId === 'string' && value.context?.client && value.playbackContext) {
        value.contentCheckOk = true
        value.racyCheckOk = true
      }
    } catch {}
    return stringify.call(this, value, ...rest)
  }

  // --- Only the picture ------------------------------------------------------------------------------------------
  //
  // The page is hidden under the cab, shown only where Now Playing shows a music video's picture (the host places
  // it there): everything but the video invisible, the video over the whole page, all of it shown (bars where its
  // shape isn't the page's).
  const style = document.createElement('style')
  style.textContent = `
    html, body { background: #000 !important; overflow: hidden !important; }
    body * { visibility: hidden !important; }
    #movie_player, #movie_player .html5-video-container, #movie_player video { visibility: visible !important; }
    #movie_player { position: fixed !important; inset: 0 !important; width: 100vw !important; height: 100vh !important;
      z-index: 2147483647 !important; background: #000 !important; }
    #movie_player .html5-video-container, #movie_player video.html5-main-video { position: absolute !important;
      inset: 0 !important; width: 100vw !important; height: 100vh !important; left: 0 !important; top: 0 !important;
      object-fit: contain !important; }
    /* The player's page (ytmusic-player-page) carries a transform: any ancestor with one becomes what "fixed" is
       placed against, and the player sat at its corner (240, 64), the picture shifted and cut. None of them. */
    body *:has(#movie_player) { transform: none !important; translate: none !important; scale: none !important;
      rotate: none !important; filter: none !important; perspective: none !important; contain: none !important;
      will-change: auto !important; container-type: normal !important; }
  `
  const addStyle = () => {
    const parent = document.head || document.documentElement
    if (parent) parent.appendChild(style)
    else requestAnimationFrame(addStyle)
  }
  addStyle()

  // --- How long playing takes, step by step: for the host's log, the kinds of steps and their times only
  // (YouTube's requests by kind, the player's and the media element's own events), never addresses or contents.
  const steps = { since: null, list: [], media: [] }
  const step = (what) => {
    if (steps.since !== null && steps.list.length < 60) {
      steps.list.push(`${what} ${Math.round(performance.now() - steps.since)}`)
    }
  }
  const REQUESTS = [
    [/\/youtubei\/v1\/player/, 'player'],
    [/\/youtubei\/v1\/next/, 'next'],
    [/\/videoplayback/, 'media'],
  ]
  const kindOf = (url) => REQUESTS.find(([pattern]) => pattern.test(String(url)))?.[1] ?? null
  const pageFetch = window.fetch.bind(window)
  window.fetch = function (input, init) {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : (input?.url ?? '')
    const kind = steps.since === null ? null : kindOf(url)
    const asked = pageFetch(input, init)
    if (kind) {
      step(`${kind}>`)
      asked.then(
        (response) => step(response.ok ? kind : `${kind}!${response.status}`),
        (error) => step(`${kind}!${error?.name ?? 'error'}`),
      )
    }
    return asked
  }
  const pageOpen = XMLHttpRequest.prototype.open
  XMLHttpRequest.prototype.open = function (method, url, ...rest) {
    const kind = steps.since === null ? null : kindOf(url)
    if (kind) {
      step(`${kind}>`)
      this.addEventListener(
        'loadend',
        () => step(this.status >= 200 && this.status < 400 ? kind : `${kind}!${this.status}`),
        {
          once: true,
        },
      )
    }
    return pageOpen.call(this, method, url, ...rest)
  }
  const MEDIA_EVENTS = ['loadstart', 'loadedmetadata', 'canplay', 'playing', 'waiting', 'stalled']
  const startSteps = () => {
    steps.since = performance.now()
    steps.list = []
    const element = media()
    if (element) {
      for (const name of MEDIA_EVENTS) {
        const heard = () => step(name)
        element.addEventListener(name, heard)
        steps.media.push(() => element.removeEventListener(name, heard))
      }
    }
  }
  const stopSteps = () => {
    const said = steps.list.join(', ')
    steps.since = null
    steps.media.splice(0).forEach((remove) => remove())
    return said
  }

  // --- The page's own parts -----------------------------------------------------------------------------------

  const app = () => document.querySelector('ytmusic-app')
  // YouTube's player (play, pause, seek, the time, the volume).
  const player = () => {
    const element = document.getElementById('movie_player')
    return typeof element?.getPlayerState === 'function' ? element : null
  }
  // The player bar, and its own buttons (shuffle, repeat): pressed as the player presses them. The bar builds its
  // buttons in its own time (a command can come before they're there): waited for, 3 s at most.
  const bar = () => document.querySelector('ytmusic-player-bar')
  const press = async (name) => {
    for (let waited = 0; waited < 3000; waited += 50) {
      const button = bar()?.querySelector(`.${name}`)
      if (button) {
        button.click()
        return true
      }
      await new Promise((resolve) => setTimeout(resolve, 50))
    }
    return false
  }
  // The page's store: its queue (songs, the one playing, repeat, shuffle), likes, the player's state.
  const store = () => document.querySelector('ytmusic-player-queue')?.queue?.store?.store ?? null
  const state = () => {
    try {
      return store()?.getState?.() ?? null
    } catch {
      return null
    }
  }
  const media = () => document.querySelector('video.html5-main-video') ?? document.querySelector('video')
  // The player page's Song/Video switch: its `playback-mode` (`ATV_PREFERRED`, the song; `OMV_PREFERRED`, its
  // music video) and whether the song playing has a video (`selected-item-has-video`).
  const toggle = () => document.querySelector('ytmusic-av-toggle')

  const ready = new Promise((resolve) => {
    const look = () => {
      if (window.ytcfg?.get?.('INNERTUBE_CONTEXT') && app() && player() && store()) resolve()
      else setTimeout(look, 200)
    }
    look()
  })

  // --- Asking YouTube Music, as its own site asks ----------------------------------------------------------

  let language = null
  const signedIn = () => !!window.ytcfg?.get?.('LOGGED_IN')
  // Signed in, YouTube Music's own site signs its requests with a hash of its session cookie (SAPISIDHASH); so
  // do these. The cookie never leaves this page but in that hash, to YouTube, as its own site sends it.
  const ORIGIN = 'https://music.youtube.com'
  const authorization = async () => {
    if (!signedIn()) return {}
    const cookie = document.cookie
      .split('; ')
      .find((c) => c.startsWith('SAPISID=') || c.startsWith('__Secure-3PAPISID='))
    if (!cookie) return {}
    const now = Math.floor(Date.now() / 1000)
    const digest = await crypto.subtle.digest(
      'SHA-1',
      new TextEncoder().encode(`${now} ${cookie.split('=')[1]} ${ORIGIN}`),
    )
    const hash = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
    return { Authorization: `SAPISIDHASH ${now}_${hash}`, 'X-Goog-AuthUser': '0', 'X-Origin': ORIGIN }
  }
  const innertube = async (endpoint, body) => {
    await ready
    const context = structuredClone(window.ytcfg.get('INNERTUBE_CONTEXT'))
    if (language) context.client.hl = language
    const visitor = window.ytcfg.get('VISITOR_DATA')
    const response = await pageFetch(`/youtubei/v1/${endpoint}?prettyPrint=false`, {
      method: 'POST',
      credentials: 'same-origin',
      headers: {
        'Content-Type': 'application/json',
        'X-Youtube-Client-Name': String(window.ytcfg.get('INNERTUBE_CONTEXT_CLIENT_NAME') ?? 67),
        'X-Youtube-Client-Version': context.client.clientVersion,
        ...(visitor ? { 'X-Goog-Visitor-Id': visitor } : {}),
        ...(await authorization()),
      },
      body: stringify({ context, ...body }),
    })
    if (!response.ok) throw new Error(`${endpoint}: ${response.status}`)
    return json.call(response)
  }

  // --- What the app shows, from YouTube Music's answers -----------------------------------------------------

  // A text as YouTube sends it: plain, or in runs.
  const text = (t) => (typeof t === 'string' ? t : (t?.simpleText ?? t?.runs?.map((run) => run.text).join('') ?? ''))
  // A picture at about `size` pixels: Google's image hosts size it by its address (`=w60-h60-…`, `=s192`), a
  // video's thumbnail is the widest there is.
  const picture = (thumbnails, size = 544) => {
    const list = (thumbnails ?? []).filter((each) => each?.url)
    if (!list.length) return null
    let url = list.reduce((a, b) => ((b.width ?? 0) > (a.width ?? 0) ? b : a)).url
    if (url.startsWith('//')) url = `https:${url}`
    if (/googleusercontent\.com|ggpht\.com/.test(url)) {
      url = url.replace(/=w\d+-h\d+/, `=w${size}-h${size}`).replace(/=s\d+(?=[-?]|$)/, `=s${size}`)
    }
    return url
  }
  const thumbnailsOf = (renderer) =>
    renderer?.thumbnail?.musicThumbnailRenderer?.thumbnail?.thumbnails ??
    renderer?.thumbnailRenderer?.musicThumbnailRenderer?.thumbnail?.thumbnails ??
    renderer?.thumbnail?.thumbnails ??
    renderer?.thumbnail?.croppedSquareThumbnailRenderer?.thumbnail?.thumbnails ??
    null
  const PAGES = {
    MUSIC_PAGE_TYPE_ALBUM: 'album',
    MUSIC_PAGE_TYPE_AUDIOBOOK: 'album',
    MUSIC_PAGE_TYPE_PLAYLIST: 'playlist',
    MUSIC_PAGE_TYPE_ARTIST: 'artist',
    MUSIC_PAGE_TYPE_LIBRARY_ARTIST: 'artist',
    MUSIC_PAGE_TYPE_USER_CHANNEL: 'channel',
    MUSIC_PAGE_TYPE_PODCAST_SHOW_DETAIL_PAGE: 'podcast',
  }
  const VIDEOS = {
    MUSIC_VIDEO_TYPE_ATV: 'song',
    MUSIC_VIDEO_TYPE_OMV: 'video',
    MUSIC_VIDEO_TYPE_UGC: 'video',
    MUSIC_VIDEO_TYPE_OFFICIAL_SOURCE_MUSIC: 'video',
    MUSIC_VIDEO_TYPE_PODCAST_EPISODE: 'episode',
  }
  const pageTypeOf = (browse) =>
    browse?.browseEndpointContextSupportedConfigs?.browseEndpointContextMusicConfig?.pageType ?? null
  const videoTypeOf = (watch) =>
    watch?.watchEndpointMusicSupportedConfigs?.watchEndpointMusicConfig?.musicVideoType ?? null
  const browseOf = (endpoint) => {
    const browse = endpoint?.browseEndpoint
    return browse?.browseId ? { id: browse.browseId, ...(browse.params ? { params: browse.params } : {}) } : null
  }
  // Playing it: a song (from a list, at its place), or a list (shuffled, a radio: YouTube's own params).
  const watchOf = (endpoint) => {
    const watch = endpoint?.watchEndpoint
    if (watch?.videoId || watch?.playlistId) {
      return {
        ...(watch.videoId ? { video: watch.videoId } : {}),
        ...(watch.playlistId ? { playlist: watch.playlistId } : {}),
        ...(watch.params ? { params: watch.params } : {}),
        ...(typeof watch.index === 'number' ? { index: watch.index } : {}),
      }
    }
    const list = endpoint?.watchPlaylistEndpoint
    return list?.playlistId ? { playlist: list.playlistId, ...(list.params ? { params: list.params } : {}) } : null
  }
  const overlayPlay = (renderer) =>
    (renderer?.overlay ?? renderer?.thumbnailOverlay)?.musicItemThumbnailOverlayRenderer?.content
      ?.musicPlayButtonRenderer?.playNavigationEndpoint ?? null
  // The first of a text's runs that goes to a page of this kind (a song's artist, its album).
  const runTo = (texts, kinds) => {
    for (const t of texts) {
      for (const run of t?.runs ?? []) {
        if (kinds.includes(PAGES[pageTypeOf(run.navigationEndpoint?.browseEndpoint)]))
          return browseOf(run.navigationEndpoint)
      }
    }
    return null
  }
  const explicitIn = (badges) =>
    (badges ?? []).some((badge) => badge?.musicInlineBadgeRenderer?.icon?.iconType === 'MUSIC_EXPLICIT_BADGE')
  // An ARGB colour as YouTube writes it (a number), as `#rrggbb`.
  const colorOf = (argb) => {
    const value = Number(argb)
    return Number.isFinite(value) && value > 0 ? `#${(value & 0xffffff).toString(16).padStart(6, '0')}` : null
  }

  // A card (an album's, a playlist's, an artist's, a video's), side by side with others.
  const fromCard = (r) => {
    const browse = browseOf(r.navigationEndpoint)
    const play = watchOf(overlayPlay(r)) ?? watchOf(r.navigationEndpoint)
    const watch = r.navigationEndpoint?.watchEndpoint
    const kind =
      PAGES[pageTypeOf(r.navigationEndpoint?.browseEndpoint)] ?? VIDEOS[videoTypeOf(watch)] ?? (watch ? 'video' : null)
    if (!kind) return null
    return {
      kind,
      title: text(r.title),
      subtitle: text(r.subtitle),
      image: picture(thumbnailsOf(r)),
      ...(browse ? { browse } : {}),
      ...(play ? { play } : {}),
      ...(watch?.videoId ? { video: watch.videoId } : {}),
      ...(kind === 'song' || kind === 'video'
        ? { artist: runTo([r.subtitle], ['artist', 'channel']) ?? undefined }
        : {}),
      explicit: explicitIn(r.subtitleBadges),
    }
  }
  // A row (a song's, or anything a search or a list finds): its columns of text, its length, its play.
  const fromRow = (r) => {
    const columns = (r.flexColumns ?? []).map((c) => c.musicResponsiveListItemFlexColumnRenderer?.text)
    const titleRun = columns[0]?.runs?.[0]
    const play =
      watchOf(overlayPlay(r)) ?? watchOf(titleRun?.navigationEndpoint) ?? watchOf(r.navigationEndpoint) ?? null
    const watchEndpoint =
      overlayPlay(r)?.watchEndpoint ??
      titleRun?.navigationEndpoint?.watchEndpoint ??
      r.navigationEndpoint?.watchEndpoint
    const browse = browseOf(r.navigationEndpoint)
    const video = r.playlistItemData?.videoId ?? watchEndpoint?.videoId ?? null
    const kind =
      (video ? (VIDEOS[videoTypeOf(watchEndpoint)] ?? 'song') : null) ??
      PAGES[pageTypeOf(r.navigationEndpoint?.browseEndpoint)] ??
      null
    if (!kind) return null
    return {
      kind,
      title: text(columns[0]),
      subtitle: columns.slice(1).map(text).filter(Boolean).join(' • '),
      image: picture(thumbnailsOf(r), 226),
      ...(browse ? { browse } : {}),
      ...(play ? { play } : {}),
      ...(video ? { video } : {}),
      ...(video ? { artist: runTo(columns.slice(1), ['artist', 'channel']) ?? undefined } : {}),
      ...(video ? { album: runTo(columns.slice(1), ['album']) ?? undefined } : {}),
      duration: text(r.fixedColumns?.[0]?.musicResponsiveListItemFixedColumnRenderer?.text),
      explicit: explicitIn(r.badges),
      rank: text(r.index) || text(r.customIndexColumn?.musicCustomIndexColumnRenderer?.text),
    }
  }
  // A mood's or genre's button (or Explore's own: New releases, Charts, Moods & genres).
  const fromButton = (r) => {
    const browse = browseOf(r.clickCommand)
    return browse
      ? {
          kind: 'mood',
          title: text(r.buttonText),
          subtitle: '',
          browse,
          color: colorOf(r.solid?.leftStripeColor) ?? undefined,
        }
      : null
  }
  // A podcast's episode, as Home lists them.
  const fromEpisode = (r) => {
    const play = watchOf(r.onTap) ?? watchOf(overlayPlay(r))
    return play
      ? {
          kind: 'episode',
          title: text(r.title),
          subtitle: text(r.subtitle),
          image: picture(thumbnailsOf(r)),
          play,
          ...(play.video ? { video: play.video } : {}),
        }
      : null
  }
  const item = (content) => {
    try {
      if (content?.musicTwoRowItemRenderer) return fromCard(content.musicTwoRowItemRenderer)
      if (content?.musicResponsiveListItemRenderer) return fromRow(content.musicResponsiveListItemRenderer)
      if (content?.musicNavigationButtonRenderer) return fromButton(content.musicNavigationButtonRenderer)
      if (content?.musicMultiRowListItemRenderer) return fromEpisode(content.musicMultiRowListItemRenderer)
    } catch {
      // One that doesn't read is left out.
    }
    return null
  }
  const items = (contents) => (contents ?? []).map(item).filter((each) => each?.title)

  // A page's shelves, as YouTube Music lays them out.
  const sections = (contents) => {
    const found = []
    for (const content of contents ?? []) {
      const carousel = content.musicCarouselShelfRenderer ?? content.musicImmersiveCarouselShelfRenderer
      if (carousel) {
        const header =
          carousel.header?.musicCarouselShelfBasicHeaderRenderer ??
          carousel.header?.musicImmersiveCarouselShelfBasicHeaderRenderer
        const list = items(carousel.contents)
        if (!list.length) continue
        const rows = carousel.contents?.[0]?.musicResponsiveListItemRenderer
        const buttons = carousel.contents?.[0]?.musicNavigationButtonRenderer
        const more = browseOf(header?.moreContentButton?.buttonRenderer?.navigationEndpoint)
        found.push({
          title: text(header?.title),
          strapline: text(header?.strapline),
          style: buttons ? 'moodRows' : rows ? 'columns' : 'shelf',
          items: list,
          ...(more ? { more } : {}),
        })
        continue
      }
      const shelf = content.musicShelfRenderer ?? content.musicPlaylistShelfRenderer
      if (shelf) {
        const list = items(shelf.contents)
        if (!list.length) continue
        const more = browseOf(shelf.bottomEndpoint)
        found.push({ title: text(shelf.title), style: 'songs', items: list, ...(more ? { more } : {}) })
        continue
      }
      const grid = content.gridRenderer
      if (grid) {
        const list = items(grid.items)
        if (!list.length) continue
        const buttons = grid.items?.[0]?.musicNavigationButtonRenderer
        found.push({
          title: text(grid.header?.gridHeaderRenderer?.title),
          style: buttons ? 'moods' : 'shelf',
          items: list,
        })
        continue
      }
      const about = content.musicDescriptionShelfRenderer
      if (about) {
        const words = text(about.description)
        if (words) found.push({ title: text(about.header), style: 'about', items: [], text: words })
        continue
      }
      if (content.itemSectionRenderer) found.push(...sections(content.itemSectionRenderer.contents))
    }
    return found
  }
  const chipsOf = (header) =>
    (header?.chipCloudRenderer?.chips ?? [])
      .map((chip) => chip.chipCloudChipRenderer)
      .filter(Boolean)
      .map((chip) => {
        const params =
          chip.navigationEndpoint?.browseEndpoint?.params ?? chip.navigationEndpoint?.searchEndpoint?.params
        return { label: text(chip.text), ...(params ? { params } : {}), selected: Boolean(chip.isSelected) }
      })
      .filter((chip) => chip.label)
  // The token for more of a list, wherever YouTube puts it.
  const moreOf = (list) =>
    list?.continuations?.[0]?.nextContinuationData?.continuation ??
    list?.contents?.at?.(-1)?.continuationItemRenderer?.continuationEndpoint?.continuationCommand?.token ??
    null

  // A page as YouTube Music answers it (one column, or a header and a column beside it), as shelves.
  const pageOf = (found) => {
    const single =
      found?.contents?.singleColumnBrowseResultsRenderer?.tabs?.[0]?.tabRenderer?.content?.sectionListRenderer
    const double = found?.contents?.twoColumnBrowseResultsRenderer
    const list = single ?? double?.secondaryContents?.sectionListRenderer ?? found?.contents?.sectionListRenderer
    const header = found?.header?.musicHeaderRenderer ?? found?.header?.musicImmersiveHeaderRenderer
    // The picture YouTube Music shows behind its page's top (Home's, as its mood is).
    const background = picture(found?.background?.musicThumbnailRenderer?.thumbnail?.thumbnails, 1280)
    return {
      title: text(header?.title),
      ...(background ? { background } : {}),
      chips: chipsOf(list?.header),
      sections: sections(list?.contents),
      ...(moreOf(list) ? { more: moreOf(list) } : {}),
    }
  }
  // More of a page: its next shelves.
  const morePage = (found) => {
    const list = found?.continuationContents?.sectionListContinuation
    return { chips: [], sections: sections(list?.contents), ...(moreOf(list) ? { more: moreOf(list) } : {}) }
  }

  const home = async ({ chip, more }) => {
    if (more) return morePage(await innertube('browse', { continuation: more }))
    return pageOf(await innertube('browse', { browseId: 'FEmusic_home', ...(chip ? { params: chip } : {}) }))
  }
  const explore = async () => pageOf(await innertube('browse', { browseId: 'FEmusic_explore' }))
  const browse = async ({ page }) =>
    pageOf(await innertube('browse', { browseId: page.id, ...(page.params ? { params: page.params } : {}) }))

  // A search: its best match (a card, with its songs), then everything else, one row each; or one kind only.
  const search = async ({ query, filter, more }) => {
    if (more) {
      const found = await innertube('search', { continuation: more })
      const list = found?.continuationContents?.musicShelfContinuation
      return { topSongs: [], chips: [], items: items(list?.contents), ...(moreOf(list) ? { more: moreOf(list) } : {}) }
    }
    const found = await innertube('search', { query, ...(filter ? { params: filter } : {}) })
    const list = found?.contents?.tabbedSearchResultsRenderer?.tabs?.[0]?.tabRenderer?.content?.sectionListRenderer
    let top = null
    let topSongs = []
    const rows = []
    let token = null
    for (const content of list?.contents ?? []) {
      const card = content.musicCardShelfRenderer
      if (card) {
        const browse = browseOf(card.title?.runs?.[0]?.navigationEndpoint) ?? browseOf(card.onTap)
        const play =
          watchOf(card.onTap) ?? watchOf(card.title?.runs?.[0]?.navigationEndpoint) ?? watchOf(overlayPlay(card))
        const watch = card.onTap?.watchEndpoint ?? card.title?.runs?.[0]?.navigationEndpoint?.watchEndpoint
        const kind =
          PAGES[pageTypeOf((card.title?.runs?.[0]?.navigationEndpoint ?? card.onTap)?.browseEndpoint)] ??
          (watch ? (VIDEOS[videoTypeOf(watch)] ?? 'song') : null)
        if (kind) {
          top = {
            kind,
            title: text(card.title),
            subtitle: text(card.subtitle),
            image: picture(thumbnailsOf(card)),
            ...(browse ? { browse } : {}),
            ...(play ? { play } : {}),
            ...(watch?.videoId ? { video: watch.videoId } : {}),
          }
        }
        topSongs = items(card.contents)
        continue
      }
      const shelf = content.musicShelfRenderer
      if (shelf) {
        rows.push(...items(shelf.contents))
        token = moreOf(shelf) ?? token
        continue
      }
      if (content.itemSectionRenderer) rows.push(...items(content.itemSectionRenderer.contents))
    }
    return {
      ...(top ? { top } : {}),
      topSongs,
      chips: chipsOf(list?.header),
      items: rows,
      ...(token ? { more: token } : {}),
    }
  }

  const suggest = async ({ query }) => {
    if (!query.trim()) return { texts: [], items: [] }
    const found = await innertube('music/get_search_suggestions', { input: query })
    const texts = []
    const rows = []
    for (const part of found?.contents ?? []) {
      for (const content of part.searchSuggestionsSectionRenderer?.contents ?? []) {
        const words = text(content.searchSuggestionRenderer?.suggestion)
        if (words) texts.push(words)
        else {
          const row = item(content)
          if (row?.title) rows.push(row)
        }
      }
    }
    return { texts, items: rows }
  }

  // An album's or playlist's page: its header (what it is, its lines, its cover, its words, playing,
  // shuffling, its radio, whether it's in the Library), its songs, and more of them.
  const collection = async ({ id, more }) => {
    if (more) {
      const found = await innertube('browse', { continuation: more })
      const list =
        found?.onResponseReceivedActions?.[0]?.appendContinuationItemsAction?.continuationItems ??
        found?.continuationContents?.musicPlaylistShelfContinuation?.contents ??
        []
      const token =
        list.at(-1)?.continuationItemRenderer?.continuationEndpoint?.continuationCommand?.token ??
        found?.continuationContents?.musicPlaylistShelfContinuation?.continuations?.[0]?.nextContinuationData
          ?.continuation
      return { kind: 'playlist', title: '', tracks: items(list), sections: [], ...(token ? { more: token } : {}) }
    }
    const found = await innertube('browse', { browseId: id })
    const double = found?.contents?.twoColumnBrowseResultsRenderer
    const head =
      double?.tabs?.[0]?.tabRenderer?.content?.sectionListRenderer?.contents?.[0]?.musicResponsiveHeaderRenderer ??
      double?.tabs?.[0]?.tabRenderer?.content?.sectionListRenderer?.contents?.[0]
        ?.musicEditablePlaylistDetailHeaderRenderer?.header?.musicResponsiveHeaderRenderer ??
      found?.header?.musicDetailHeaderRenderer
    const secondary = double?.secondaryContents?.sectionListRenderer?.contents ?? []
    const shelf = secondary.map((c) => c.musicShelfRenderer ?? c.musicPlaylistShelfRenderer).find(Boolean)
    let play = null
    let shuffle = null
    let radio = null
    let saved = null
    for (const button of head?.buttons ?? []) {
      if (button.musicPlayButtonRenderer) play = watchOf(button.musicPlayButtonRenderer.playNavigationEndpoint)
      if (button.toggleButtonRenderer) saved = Boolean(button.toggleButtonRenderer.isToggled)
      for (const entry of button.menuRenderer?.items ?? []) {
        const menu = entry.menuNavigationItemRenderer
        const icon = menu?.icon?.iconType
        if (icon === 'MUSIC_SHUFFLE') shuffle = watchOf(menu.navigationEndpoint)
        if (icon === 'MIX') radio = watchOf(menu.navigationEndpoint)
      }
    }
    const kind = id.startsWith('MPRE') ? 'album' : 'playlist'
    const by = runTo([head?.straplineTextOne], ['artist', 'channel', 'album'])
    return {
      kind,
      title: text(head?.title),
      subtitle: text(head?.subtitle),
      second: text(head?.secondSubtitle),
      by: text(head?.straplineTextOne),
      ...(by ? { byPage: by } : {}),
      image: picture(thumbnailsOf(head) ?? head?.thumbnail?.musicThumbnailRenderer?.thumbnail?.thumbnails),
      description: text(head?.description?.musicDescriptionShelfRenderer?.description),
      ...(play ? { play } : {}),
      ...(shuffle ? { shuffle } : {}),
      ...(radio ? { radio } : {}),
      ...(play?.playlist ? { playlist: play.playlist } : {}),
      ...(signedIn() && saved !== null ? { saved } : {}),
      tracks: items(shelf?.contents),
      sections: sections(secondary.filter((c) => !c.musicShelfRenderer && !c.musicPlaylistShelfRenderer)),
      ...(moreOf(shelf) ? { more: moreOf(shelf) } : {}),
    }
  }

  const artist = async ({ id }) => {
    const found = await innertube('browse', { browseId: id })
    const head = found?.header?.musicImmersiveHeaderRenderer ?? found?.header?.musicVisualHeaderRenderer
    const list =
      found?.contents?.singleColumnBrowseResultsRenderer?.tabs?.[0]?.tabRenderer?.content?.sectionListRenderer
    const shuffle = watchOf(head?.playButton?.buttonRenderer?.navigationEndpoint)
    const radio = watchOf(head?.startRadioButton?.buttonRenderer?.navigationEndpoint)
    return {
      name: text(head?.title),
      description: text(head?.description),
      image: picture(
        thumbnailsOf(head) ?? head?.foregroundThumbnail?.musicThumbnailRenderer?.thumbnail?.thumbnails,
        1440,
      ),
      subscribers: text(head?.subscriptionButton?.subscribeButtonRenderer?.subscriberCountText),
      ...(shuffle ? { shuffle } : {}),
      ...(radio ? { radio } : {}),
      sections: sections(list?.contents),
    }
  }

  // The Library's lists (signed in), each YouTube Music's own page of it.
  const SHELVES = {
    playlists: 'FEmusic_liked_playlists',
    songs: 'FEmusic_liked_videos',
    albums: 'FEmusic_liked_albums',
    artists: 'FEmusic_library_corpus_track_artists',
    subscriptions: 'FEmusic_library_corpus_artists',
    history: 'FEmusic_history',
  }
  const library = async ({ shelf }) => {
    if (!signedIn()) return { chips: [], sections: [] }
    return pageOf(await innertube('browse', { browseId: SHELVES[shelf] ?? SHELVES.playlists }))
  }

  // A song's watch page's tabs (Up next, Lyrics, Comments, Related): the page each of the latter two opens.
  const tabsOf = async (video) => {
    const found = await innertube('next', { videoId: video, isAudioOnly: true })
    const tabs =
      found?.contents?.singleColumnMusicWatchNextResultsRenderer?.tabbedRenderer?.watchNextTabbedResultsRenderer
        ?.tabs ?? []
    const page = (prefix) =>
      tabs
        .map((tab) => tab.tabRenderer)
        .find((tab) => !tab?.unselectable && tab?.endpoint?.browseEndpoint?.browseId?.startsWith(prefix))?.endpoint
        ?.browseEndpoint?.browseId ?? null
    return { lyrics: page('MPLYt'), related: page('MPTRt') }
  }
  const lyrics = async ({ video }) => {
    const { lyrics: page } = await tabsOf(video)
    if (!page) return { text: '', source: '' }
    const found = await innertube('browse', { browseId: page })
    const shelf = (found?.contents?.sectionListRenderer?.contents ?? [])
      .map((c) => c.musicDescriptionShelfRenderer)
      .find(Boolean)
    return { text: text(shelf?.description), source: text(shelf?.footer) }
  }
  const related = async ({ video }) => {
    const { related: page } = await tabsOf(video)
    if (!page) return { chips: [], sections: [] }
    const found = await innertube('browse', { browseId: page })
    return { chips: [], sections: sections(found?.contents?.sectionListRenderer?.contents) }
  }

  const QUESTIONS = { home, explore, browse, search, suggest, collection, artist, library, lyrics, related }
  // An answer, asked of YouTube Music again once should it fail (a moment's trouble: a dropped connection);
  // failed only then.
  const answer = async (id, question) => {
    const asked = QUESTIONS[question?.ask]
    if (!asked) {
      post({ answer: id, failed: true })
      return
    }
    for (const wait of [0, 600]) {
      if (wait) await new Promise((resolve) => setTimeout(resolve, wait))
      try {
        post({ answer: id, [question.ask]: await asked(question) })
        return
      } catch {
        // Once more, or failed.
      }
    }
    post({ answer: id, failed: true })
  }

  // --- Playing --------------------------------------------------------------------------------------------------

  // YouTube's player's states.
  const UNSTARTED = -1
  const ENDED = 0
  const PLAYING = 1
  const PAUSED = 2
  const BUFFERING = 3
  const CUED = 5
  const videoNow = () => {
    try {
      return player()?.getVideoData?.()?.video_id || null
    } catch {
      return null
    }
  }
  const playerState = () => {
    try {
      return player()?.getPlayerState?.() ?? UNSTARTED
    } catch {
      return UNSTARTED
    }
  }
  // Its sound coming: the media element playing, its time moving.
  const audible = () => {
    const element = media()
    return Boolean(element && !element.paused && element.currentTime > 0.05 && element.readyState >= 3)
  }

  // Whether the player wants it playing: pressed play (or a song, next, previous), not pause. Between songs
  // (one ended, the next not started), and while a song loads, it's said as playing, waiting for its sound.
  let wanted = false
  // The song that has sounded (loading is only a song's wait for its first sound: played on after a pause, or
  // sought, its sound is back within a moment, and YouTube Music's own player shows no waiting for that).
  let sounded = null
  // Since when the player has been between songs (ended, not started, cued) while wanted: a moment of it is the
  // way to the next song; long, it's stopped (the queue's end).
  let between = null
  const BETWEEN_MOST = 4000
  const isPlaying = () => {
    const now = playerState()
    if (now === PLAYING || now === BUFFERING) return true
    if (now === PAUSED) return false
    if (!wanted) return false
    between ??= performance.now()
    return performance.now() - between < BETWEEN_MOST
  }
  const isLoading = () => {
    const video = videoNow()
    if (playerState() === PLAYING && audible()) sounded = video
    return isPlaying() && video !== sounded
  }
  // While it loads, looked at often: said again the moment its sound comes.
  let loadingWatch = null
  const watchLoading = () => {
    if (loadingWatch) return
    const since = performance.now()
    loadingWatch = setInterval(() => {
      if (isLoading() && performance.now() - since < 30000) return
      clearInterval(loadingWatch)
      loadingWatch = null
      tellPlayback()
    }, 120)
  }

  // A change of song under way (a play, a skip, a jump): the player passes through no song, unstarted and
  // buffering on the way; none of that is said, only where it ends up (the new song, playing or loading).
  let changing = 0
  const settle = async (change) => {
    changing++
    try {
      await change()
    } finally {
      changing--
      tellPlayback()
    }
  }
  // Until the song isn't `before` any more and the page's queue has it (what's said of it: its title, its
  // artists), or 6 s on.
  const songChanged = (before) =>
    new Promise((resolve) => {
      const since = performance.now()
      const look = setInterval(() => {
        const now = videoNow()
        const known = now && now !== before && queueItems().some((entry) => panelOf(entry, now))
        if (known || performance.now() - since > 6000) {
          clearInterval(look)
          resolve()
        }
      }, 40)
    })

  // How long a play took: until the page had the song, and until its sound came; said to the host, which logs
  // it (times only).
  const timePlay = (started, queued, action, before) => {
    const done = (sounding) => post({ timing: { action, queued, sounding, steps: stopSteps() } })
    const check = () => {
      const moved = before === null || videoNow() !== before
      if (moved && playerState() === PLAYING && audible()) {
        clearInterval(looking)
        clearTimeout(giveUp)
        done(Math.round(performance.now() - started))
      }
    }
    const looking = setInterval(check, 30)
    const giveUp = setTimeout(() => {
      clearInterval(looking)
      done(null)
    }, 20000)
    check()
  }

  // The page's own move to a song or a list (as a click on it is): its queue, its radio, its autoplay.
  const navigate = (endpoint) =>
    app()?.dispatchEvent(new CustomEvent('yt-navigate', { bubbles: true, composed: true, detail: { endpoint } }))
  const endpointOf = (watch) =>
    watch.video
      ? {
          watchEndpoint: {
            videoId: watch.video,
            ...(watch.playlist ? { playlistId: watch.playlist } : {}),
            ...(watch.params ? { params: watch.params } : {}),
            ...(typeof watch.index === 'number' ? { index: watch.index } : {}),
          },
        }
      : { watchPlaylistEndpoint: { playlistId: watch.playlist, ...(watch.params ? { params: watch.params } : {}) } }

  // A song changed by the page's own hand (a play, a skip, a jump), timed.
  const change = (action, move) =>
    settle(async () => {
      await ready
      // Back, late in a song, is its start again (YouTube Music's own Back): not a change of song.
      if (action === 'previous' && (Number(player()?.getCurrentTime?.()) || 0) > 3) {
        player()?.seekTo(0, true)
        return
      }
      const started = performance.now()
      const before = videoNow()
      wanted = true
      sounded = null
      between = null
      startSteps()
      move()
      await songChanged(before)
      timePlay(started, Math.round(performance.now() - started), action, before)
    })

  const queueItems = () => state()?.queue?.items ?? []
  // A queue entry's song: the entry's own, or (an entry with a song and its music video) whichever of the two
  // is `video` (the one playing), else the one the page shows first. With `video` and neither is it, none.
  const panelOf = (entry, video = null) => {
    const own = entry?.playlistPanelVideoRenderer
    const wrapper = entry?.playlistPanelVideoWrapperRenderer
    const both = [
      own ?? wrapper?.primaryRenderer?.playlistPanelVideoRenderer,
      ...(wrapper?.counterpart ?? []).map((each) => each?.counterpartRenderer?.playlistPanelVideoRenderer),
    ].filter(Boolean)
    if (video) return both.find((each) => each.videoId === video) ?? null
    return both[0] ?? null
  }

  // The volume the cab asked for (0 to 100; `null`: the page's own). YouTube Music puts back its own saved
  // volume as each song loads: set again then, and kept in the page's own saved volume too.
  let volume = null
  const keepVolume = () => {
    const p = player()
    if (volume === null || !p) return
    if (Number(p.getVolume?.()) !== volume) p.setVolume(volume)
    if (p.isMuted?.()) p.unMute()
    try {
      const now = Date.now()
      localStorage.setItem(
        'yt-player-volume',
        stringify({ data: stringify({ volume, muted: false }), expiration: now + 30 * 86400000, creation: now }),
      )
    } catch {
      // Not kept: set again as each song loads all the same.
    }
  }

  const CONTROLS = {
    play: ({ watch }) => {
      if (watch) return change('play', () => navigate(endpointOf(watch)))
      wanted = true
      between = null
      return ready.then(() => player()?.playVideo())
    },
    pause: async () => {
      await ready
      wanted = false
      player()?.pauseVideo()
    },
    // The player's own next and previous, which go through the page's queue (as its bar's buttons do, which
    // aren't always there yet).
    next: () => change('next', () => player()?.nextVideo()),
    // Within a song's first seconds, the one before; later, its start (YouTube Music's own Back).
    previous: () => change('previous', () => player()?.previousVideo()),
    seek: async ({ seconds }) => {
      await ready
      player()?.seekTo(Math.max(0, Number(seconds) || 0), true)
      tellPlayback()
    },
    volume: async ({ percent }) => {
      await ready
      volume = Math.round(Math.min(100, Math.max(0, Number(percent) || 0)))
      keepVolume()
      tellPlayback()
    },
    // The player bar's own Shuffle, when it isn't as asked. YouTube Music's web player shuffles its whole queue
    // and plays it from its first song (and, shuffle off, the queue as it was from its first): a change of song.
    shuffle: async ({ on }) => {
      await ready
      if (Boolean(state()?.queue?.shuffleEnabled) === Boolean(on)) return tellPlayback()
      return change('shuffle', () => press('shuffle'))
    },
    // The player bar's own Repeat goes off, all, one: pressed until it's as asked.
    repeat: async ({ mode }) => {
      await ready
      const want = { off: 'NONE', all: 'ALL', one: 'ONE' }[mode] ?? 'NONE'
      for (let pressed = 0; pressed < 3 && (state()?.queue?.repeatMode ?? 'NONE') !== want; pressed++) {
        if (!(await press('repeat'))) break
        await new Promise((resolve) => setTimeout(resolve, 60))
      }
      tellPlayback()
    },
    // A song of the queue: the queue's own move to it, as a click on it is.
    jump: ({ index }) => {
      const endpoint = panelOf(queueItems()[Number(index)])?.navigationEndpoint
      return endpoint ? change('jump', () => navigate(endpoint)) : Promise.resolve()
    },
    like: async ({ video, like }) => {
      const endpoint = { like: 'like/like', dislike: 'like/dislike' }[like] ?? 'like/removelike'
      await innertube(endpoint, { target: { videoId: video } })
      likes.set(video, like)
      tellPlayback()
    },
    save: async ({ playlist, on }) => {
      await innertube(on ? 'like/like' : 'like/removelike', { target: { playlistId: playlist } })
    },
    // A song into the queue, after the one playing or at its end, as the page's own ⋮ menu puts it there (Play
    // next, Add to queue): its queue entry asked of YouTube Music, then added to the page's own queue.
    queue: async ({ video, next }) => {
      await ready
      const found = await innertube('music/get_queue', { videoIds: [video] })
      const entry = found?.queueDatas?.[0]?.content
      const now = state()?.queue
      if (!entry || !now) throw new Error('not queued')
      if (typeof found.queueContextParams === 'string') {
        store().dispatch({ type: 'SET_QUEUE_CONTEXT_PARAMS', payload: found.queueContextParams })
      }
      store().dispatch({
        type: 'ADD_ITEMS',
        payload: {
          nextQueueItemId: now.nextQueueItemId,
          index: next ? Math.max(0, Number(now.selectedItemIndex) || 0) + 1 : now.items.length,
          items: [entry],
          shouldAssignIds: true,
        },
      })
      tellPlayback()
    },
    // The Song/Video switch: the page's own, pressed (Song, its starting side, plays the song; Video, its music
    // video, the picture shown in Now Playing).
    video: async ({ on }) => {
      await ready
      toggle()
        ?.querySelector(on ? '.video-button' : '.song-button')
        ?.click()
      setTimeout(tellPlayback, 300)
    },
    language: ({ code }) => {
      language = code === 'tr' || code === 'en' ? code : null
      return Promise.resolve()
    },
  }

  // --- What plays -----------------------------------------------------------------------------------------------

  // Likes as changed here, by song (the page's own store says the rest).
  const likes = new Map()
  const LIKES = { LIKE: 'like', DISLIKE: 'dislike' }
  const likeOf = (video) => {
    if (likes.has(video)) return likes.get(video)
    const stored = state()?.likeStatus?.videos?.[video] ?? bar()?.likeButtonRenderer?.likeStatus
    return LIKES[stored] ?? 'none'
  }

  // A song of the queue, as the app shows it in Up next.
  const queued = (entry) => {
    const r = panelOf(entry)
    if (!r?.videoId) return null
    const watch = r.navigationEndpoint?.watchEndpoint
    return {
      kind: VIDEOS[videoTypeOf(watch)] ?? 'song',
      title: text(r.title),
      subtitle: text(r.shortBylineText) || text(r.longBylineText),
      image: picture(r.thumbnail?.thumbnails, 226),
      video: r.videoId,
      duration: text(r.lengthText),
    }
  }
  // The song playing, as Now Playing shows it: from the queue's own entry for it (its artists, album and
  // cover), the player's own length.
  const trackOf = (entry, video) => {
    const r = panelOf(entry, video)
    const watch = r?.navigationEndpoint?.watchEndpoint
    const kind = VIDEOS[videoTypeOf(watch)] ?? 'song'
    // "BLOK3 & Poizi • KAYIP PERSONA • 2026": the artists, then the album (a video's: its views).
    const parts = (r?.longBylineText?.runs ?? []).reduce(
      (all, run) => {
        if (run.text === ' • ') all.push([])
        else all.at(-1).push(run)
        return all
      },
      [[]],
    )
    const words = (runs) => runs?.map((run) => run.text).join('') ?? ''
    const albumRuns = parts.find((runs) =>
      runs.some((run) => PAGES[pageTypeOf(run.navigationEndpoint?.browseEndpoint)] === 'album'),
    )
    const data = player()?.getVideoData?.() ?? {}
    return {
      video,
      title: text(r?.title) || data.title || '',
      artists: words(parts[0]) || data.author || '',
      ...(runTo([r?.longBylineText], ['artist', 'channel'])
        ? { artist: runTo([r?.longBylineText], ['artist', 'channel']) }
        : {}),
      album: words(albumRuns),
      ...(runTo([r?.longBylineText], ['album']) ? { albumPage: runTo([r?.longBylineText], ['album']) } : {}),
      image: picture(r?.thumbnail?.thumbnails, 1080),
      duration: Number(player()?.getDuration?.()) || 0,
      kind,
      like: likeOf(video),
      // A picture to show: the page's own Song/Video switch says the song has one (a music video).
      hasVideo: Boolean(toggle()?.hasAttribute('selected-item-has-video')),
    }
  }

  let telling = Promise.resolve()
  const tellPlayback = () => {
    telling = telling
      .then(async () => {
        if (changing > 0) return
        await ready
        const p = player()
        const video = videoNow()
        const all = queueItems()
        if (!video || !p) {
          // Nothing plays only once the queue is empty (between songs, the player has none for a moment).
          if (!all.length) post({ playback: null })
          return
        }
        const at = all.findIndex((entry) => panelOf(entry, video))
        const index = at >= 0 ? at : Math.max(0, Number(state()?.queue?.selectedItemIndex) || 0)
        const start = Math.max(0, index - 20)
        const queue = all
          .slice(start, start + 100)
          .map(queued)
          .filter(Boolean)
        const loading = isLoading()
        if (loading) watchLoading()
        const now = state()
        post({
          playback: {
            track: trackOf(all[index], video),
            playing: isPlaying(),
            loading,
            position: Math.max(0, Number(p.getCurrentTime?.()) || 0),
            duration: Number(p.getDuration?.()) || 0,
            shuffle: Boolean(now?.queue?.shuffleEnabled),
            repeat: { ALL: 'all', ONE: 'one' }[now?.queue?.repeatMode] ?? 'off',
            volume: typeof p.getVolume === 'function' ? (p.isMuted?.() ? 0 : Number(p.getVolume()) / 100) : null,
            video: toggle()?.getAttribute('playback-mode') === 'OMV_PREFERRED',
            queue,
            start,
            index,
            from: text(now?.queue?.header?.subtitle),
            ad: Boolean(now?.player?.adPlaying),
          },
        })
      })
      .catch(() => {})
  }

  // --- The app's commands, and the account ----------------------------------------------------------------

  // The truck's own speakers (cab_sound.js, loaded first): the player's media element, told the speakers, the
  // head and the camera by the host.
  const cab = window.westplayCabSound?.(() => media(), post)
  if (cab) setInterval(() => cab.tick(), 250)
  const CAB = {
    speakers: (data) => cab?.speakers(data.speakers),
    head: (data) => cab?.head(data),
    outside: (data) => cab?.outside(data.on, data.distance),
    equalizer: (data) => cab?.equalizer(data),
    spectrum: (data) => cab?.spectrum(data.on),
  }

  window.chrome?.webview?.addEventListener('message', ({ data }) => {
    if (CAB[data?.action]) CAB[data.action](data)
    else if (data?.action === 'ask') answer(data.id, data.question)
    else CONTROLS[data?.action]?.(data)?.catch?.(() => {})
  })

  // The account's picture, asked once it's signed in (YouTube's account menu: its picture only).
  const avatar = { url: null, asked: false }
  const askAvatar = async () => {
    avatar.asked = true
    const found = await innertube('account/account_menu', {}).catch(() => null)
    const header =
      found?.actions?.[0]?.openPopupAction?.popup?.multiPageMenuRenderer?.header?.activeAccountHeaderRenderer
    avatar.url = picture(header?.accountPhoto?.thumbnails, 160)
    tellState(true)
  }

  // Signed in or not (and the account's picture), and playing.
  let told = ''
  const tellState = (always = false) => {
    if (!window.ytcfg?.get?.('INNERTUBE_CONTEXT')) return
    const now = signedIn()
    if (now && !avatar.asked) askAvatar()
    if (!now) Object.assign(avatar, { url: null, asked: false })
    const said = stringify({ signedIn: now, playing: isPlaying(), avatar: avatar.url })
    if (said === told && !always) return
    told = said
    post(parse(said))
  }

  // YouTube Music asks, after a while with no one touching the page, whether anyone's still listening, and
  // pauses until answered. The player is listening (the page is driven from the cab): its clock of the last
  // touch is kept fresh.
  setInterval(() => {
    window._lact = Date.now()
  }, 60000)

  ready.then(() => {
    // This page is ready (a page loaded anew: the questions it missed are asked again).
    post({ ready: true })
    const p = player()
    const STATES = {
      [UNSTARTED]: 'unstarted',
      [ENDED]: 'ended',
      [PLAYING]: 'playing',
      [PAUSED]: 'paused',
      [BUFFERING]: 'buffering',
      [CUED]: 'cued',
    }
    p.addEventListener('onStateChange', (now) => {
      step(`yt:${STATES[now] ?? now}`)
      keepVolume()
      if (now === PLAYING || now === BUFFERING || now === PAUSED) between = null
      // Paused by the page itself (its own reasons): as it is.
      if (now === PAUSED && changing === 0) wanted = false
      if (now === PLAYING) wanted = true
      tellPlayback()
      tellState()
    })
    p.addEventListener('onVolumeChange', () => {
      // The page's own volume put back over the cab's: the cab's again.
      if (volume !== null && Number(p.getVolume?.()) !== volume) setTimeout(keepVolume, 0)
      tellPlayback()
    })
    // The queue, the song, repeat and shuffle, likes: said again when what the app shows of them changes.
    let shown = ''
    store().subscribe(() => {
      const now = state()
      const said = stringify([
        now?.queue?.items?.length,
        now?.queue?.selectedItemIndex,
        now?.queue?.repeatMode,
        now?.queue?.shuffleEnabled,
        now?.player?.adPlaying,
        now?.queue?.playbackContentMode,
        text(now?.queue?.header?.subtitle),
      ])
      if (said === shown) return
      shown = said
      tellPlayback()
    })
    // Seeking moves the time: said again then (the app runs the time on by itself while it plays).
    let lastTime = 0
    let lastAt = performance.now()
    setInterval(() => {
      const time = Number(p.getCurrentTime?.()) || 0
      const expected = lastTime + (playerState() === PLAYING ? (performance.now() - lastAt) / 1000 : 0)
      if (Math.abs(time - expected) > 2) tellPlayback()
      lastTime = time
      lastAt = performance.now()
    }, 500)
    tellState(true)
    tellPlayback()
  })
  setInterval(() => tellState(), 1000)
})()
