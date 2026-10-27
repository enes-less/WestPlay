// Runs in the hidden YouTube WebView (youtube.rs) before youtube.com's own scripts: YouTube serves no ads, its
// interface is hidden so only the video shows, the host drives its player and hears how it plays, and the
// cabin page's YouTube app asks YouTube itself, through here, what to show (search, suggestions, Up next).
(() => {
  if (location.hostname !== 'www.youtube.com') return

  // No ads, and no wait for one: YouTube holds a video back for most of the length of an ad it would have
  // played (its stream server first answers "wait N ms"). The player's request for a video is written as
  // one YouTube serves with no ad and no wait (ways found by uBlock Origin's filter authors), and if YouTube
  // holds the video back anyway, the next way is tried and the video asked for again. The request is
  // compressed before it's sent, so it's changed as it's written out, before YouTube's lock-down script can
  // freeze JSON (this runs first).
  const WAYS = [
    (request, content) => {
      request.params = '8AUB'
      content.lactMilliseconds = String(Date.now())
    },
    (request) => {
      request.params = 'YAHI'
    },
    (request, content) => {
      request.playbackContext.adPlaybackContext = { adType: 'AD_TYPE_INSTREAM' }
      content.lactMilliseconds = String(Date.now())
    },
  ]
  // The way the next request is written (past the last: as YouTube writes it), and whether the last request
  // was written a way (YouTube's own move to a video, from its home page, asks without an attestation and
  // is left as it is, as uBlock leaves it: the video is then asked for again, written the way).
  let way = 0
  let rewritten = false
  // The video last asked for written a way; and one to ask for once as YouTube writes it (see the reports below).
  let writtenFor = ''
  let plain = ''
  const stringify = JSON.stringify
  JSON.stringify = function (value, ...rest) {
    try {
      // A request for a video's player data; written a way when it's the player's own (the one asking for an
      // attestation).
      const content = value?.playbackContext?.contentPlaybackContext
      if (typeof value?.videoId === 'string' && value.context?.client && content) {
        // Past YouTube's warning ("this video may contain disturbing images: I understand and want to
        // continue"), as its own continue asks: nobody is there to press it, and it stood in for songs' videos
        // (CONTENT_CHECK_REQUIRED, owner 2026-10-27).
        value.contentCheckOk = true
        value.racyCheckOk = true
        rewritten = !!value.attestationRequest && way < WAYS.length && value.videoId !== plain
        if (rewritten) {
          writtenFor = value.videoId
          WAYS[way](value, content)
          content.referer = (content.referer || location.href).replace(/(#reloadxhr)?$/, '#reloadxhr')
        }
      }
    } catch {}
    return stringify.call(this, value, ...rest)
  }

  // Whether the stream server's answer is only "wait": a request policy with a backoff (UMP part 35, field
  // 4) and no media (part 21). Such an answer is a hundred bytes or so; only its start is read.
  const umpNumber = (b, o) => {
    const first = b[o]
    if (first < 128) return [first, o + 1]
    if (first < 192) return [(first & 0x3f) + 64 * b[o + 1], o + 2]
    if (first < 224) return [(first & 0x1f) + 32 * (b[o + 1] + 256 * b[o + 2]), o + 3]
    if (first < 240) return [(first & 0x0f) + 16 * (b[o + 1] + 256 * (b[o + 2] + 256 * b[o + 3])), o + 4]
    return [b[o + 1] + 256 * (b[o + 2] + 256 * (b[o + 3] + 256 * b[o + 4])), o + 5]
  }
  const backoff = (policy) => {
    let o = 0
    const varint = () => {
      let value = 0
      let scale = 1
      let byte
      do {
        byte = policy[o++]
        value += (byte & 0x7f) * scale
        scale *= 128
      } while (byte & 0x80 && o < policy.length)
      return value
    }
    while (o < policy.length) {
      const key = varint()
      const field = Math.floor(key / 8)
      const wire = key & 7
      if (wire === 0) {
        const value = varint()
        if (field === 4) return value
      } else if (wire === 2) o += varint()
      else if (wire === 1) o += 8
      else if (wire === 5) o += 4
      else return 0
    }
    return 0
  }
  const onlyWait = (b) => {
    let wait = 0
    let o = 0
    while (o < b.length) {
      let type, size
      ;[type, o] = umpNumber(b, o)
      ;[size, o] = umpNumber(b, o)
      if (type === 21) return false
      if (type === 35) wait = backoff(b.subarray(o, o + size))
      o += size
    }
    return wait > 0
  }
  const fetch = window.fetch
  window.fetch = function (input, init) {
    const result = fetch.call(this, input, init)
    const url = typeof input === 'string' ? input : (input?.url ?? '')
    if (url.includes('/videoplayback')) {
      result
        .then(async (response) => {
          const reader = response.clone().body?.getReader()
          if (!reader) return
          const { value } = await reader.read()
          const { done } = value && value.length <= 512 ? await reader.read() : { done: false }
          reader.cancel().catch(() => {})
          if (done && value && onlyWait(value)) heldBack()
        })
        .catch(() => {})
    }
    return result
  }
  // YouTube holds the video back (only ever before it starts: a "wait" while it plays is the server saying
  // enough is buffered): asked for again, the next way if it was asked for a way already (once per way and
  // video). The way that works is kept for the videos after it.
  let retried = ''
  let played = ''
  // How often YouTube held the video back, for the log (the diagnosis below).
  let holds = 0
  const heldBack = () => {
    const p = player()
    const video = p?.getVideoData?.()?.video_id
    if (!video || video === played) return
    holds++
    if (rewritten) way++
    const attempt = `${video}:${way}`
    if (way >= WAYS.length || retried === attempt) return
    retried = attempt
    p.loadVideoById({ videoId: video, startSeconds: p.getCurrentTime?.() ?? 0 })
    setTimeout(() => p.playVideo(), 300)
  }

  // And any ad slots that come anyway taken out of the player data, wherever it comes from, as uBlock does.
  const AD_KEYS = ['adPlacements', 'adSlots', 'playerAds', 'adBreakHeartbeatParams']
  const strip = (data) => {
    if (data && typeof data === 'object') {
      for (const key of AD_KEYS) delete data[key]
      if (data.playerResponse) strip(data.playerResponse)
    }
    return data
  }
  const parse = JSON.parse
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

  // Only the video: everything else invisible, the player over the whole page (the host sizes the page to
  // where the video shows), the picture letterboxed.
  const style = document.createElement('style')
  style.textContent = `
    html, body { background: #000 !important; overflow: hidden !important; }
    body * { visibility: hidden !important; }
    #movie_player, #movie_player .html5-video-container, #movie_player video { visibility: visible !important; }
    /* And its captions, when they're on: YouTube's own, over the picture. */
    #movie_player .ytp-caption-window-container, #movie_player .ytp-caption-window-container * {
      visibility: visible !important; }
    #movie_player { position: fixed !important; inset: 0 !important; width: 100vw !important; height: 100vh !important;
      z-index: 2147483647 !important; background: #000 !important; }
    /* Every video in the player, as every one shows: one YouTube sizes itself (its own box's, from the page's top
       left) would play small in the corner (players, 2026-10-27: the next video, after the first full). */
    #movie_player .html5-video-container, #movie_player video { position: absolute !important;
      inset: 0 !important; width: 100vw !important; height: 100vh !important; left: 0 !important; top: 0 !important;
      object-fit: contain !important; }
    /* Filling a box of another shape (the Dashboard's Now Playing): cropped, no bars. */
    html.westplay-crop #movie_player video { object-fit: cover !important; }
  `
  const addStyle = () => {
    const parent = document.head || document.documentElement
    if (parent) parent.appendChild(style)
    else requestAnimationFrame(addStyle)
  }
  addStyle()

  // The host's commands, carried out on YouTube's own player (commands before it's there wait). The page
  // opens on YouTube's home page, with no video: the first video is YouTube's own move to it, as a click on
  // it would be, and every video after it a load on the player that's then there. Either way the player
  // asks for the video, and its request says "no ads".
  const player = () => {
    const element = document.getElementById('movie_player')
    return element?.loadVideoById && location.pathname === '/watch' ? element : null
  }
  // YouTube's app, to move to a video when there's no player yet: once its first page has finished (a move
  // before then is undone as the home page finishes).
  let settled = false
  document.addEventListener('yt-navigate-finish', () => (settled = true))
  const app = () => (settled ? document.querySelector('ytd-app') : null)
  const watch = (video, start) => {
    const url = `/watch?v=${video}${start ? `&t=${Math.floor(start)}s` : ''}`
    app().dispatchEvent(
      new CustomEvent('yt-navigate', {
        bubbles: true,
        composed: true,
        detail: {
          endpoint: {
            commandMetadata: { webCommandMetadata: { url, webPageType: 'WEB_PAGE_TYPE_WATCH', rootVe: 3832 } },
            watchEndpoint: start ? { videoId: video, startTimeSeconds: Math.floor(start) } : { videoId: video },
          },
        },
      }),
    )
  }
  // What the cabin page asks of YouTube: search, suggestions, Up next. Asked as YouTube's own site asks, from
  // its own page (its identity, and the player's sign-in when there is one), in the cabin's language; the
  // answers cut down to the few fields of a video the page shows. `{answer: id, ...}`; `failed` when YouTube
  // didn't answer.
  let language = null
  const DATA = new Set(['search', 'suggest', 'related', 'feed', 'channel', 'subscribe', 'playlist'])
  const config = () => window.ytcfg?.get?.('INNERTUBE_CONTEXT') ?? null
  const signedIn = () => !!window.ytcfg?.get?.('LOGGED_IN')
  // Signed in, YouTube's own site signs its requests with a hash of its session cookie (SAPISIDHASH); so do
  // these. The cookie never leaves this page but in that hash, to YouTube, as YouTube's own site sends it.
  const authorization = async () => {
    if (!signedIn()) return {}
    const cookie = document.cookie.split('; ').find((c) => c.startsWith('SAPISID=') || c.startsWith('__Secure-3PAPISID='))
    if (!cookie) return {}
    const now = Math.floor(Date.now() / 1000)
    const origin = 'https://www.youtube.com'
    const digest = await crypto.subtle.digest('SHA-1', new TextEncoder().encode(`${now} ${cookie.split('=')[1]} ${origin}`))
    const hash = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
    return { Authorization: `SAPISIDHASH ${now}_${hash}`, 'X-Goog-AuthUser': '0', 'X-Origin': origin }
  }
  const innertube = async (endpoint, body) => {
    const context = structuredClone(config())
    if (language) context.client.hl = language
    const response = await fetch(`/youtubei/v1/${endpoint}?prettyPrint=false`, {
      method: 'POST',
      credentials: 'same-origin',
      headers: {
        'Content-Type': 'application/json',
        'X-Youtube-Client-Name': '1',
        'X-Youtube-Client-Version': context.client.clientVersion,
        ...(await authorization()),
      },
      body: stringify({ context, ...body }),
    })
    if (!response.ok) throw new Error(`${endpoint}: ${response.status}`)
    return response.json()
  }
  // A text as YouTube sends it: plain, in runs, or a view model's.
  const text = (t) => t?.simpleText ?? t?.runs?.map((run) => run.text).join('') ?? t?.content ?? ''
  // The widest of a set of images, up to `most` pixels wide (the narrowest when all are wider).
  const image = (sources, most = 720) => {
    const fitting = (sources ?? []).filter((source) => source.url && (source.width ?? 0) <= most)
    const pick = fitting.length
      ? fitting.reduce((a, b) => ((b.width ?? 0) > (a.width ?? 0) ? b : a))
      : (sources ?? []).find((source) => source.url)
    const url = pick?.url ?? null
    return url?.startsWith('//') ? `https:${url}` : url
  }
  // A search result.
  const fromVideoRenderer = (v) => {
    const duration = text(v.lengthText)
    const live =
      !duration &&
      ((v.badges ?? []).some((b) => b.metadataBadgeRenderer?.style === 'BADGE_STYLE_TYPE_LIVE_NOW') ||
        (v.thumbnailOverlays ?? []).some((o) => o.thumbnailOverlayTimeStatusRenderer?.style === 'LIVE'))
    return {
      video: v.videoId,
      title: text(v.title),
      channel: text(v.ownerText ?? v.longBylineText ?? v.shortBylineText),
      avatar: image(
        v.avatar?.decoratedAvatarViewModel?.avatar?.avatarViewModel?.image?.sources ??
          v.channelThumbnailSupportedRenderers?.channelThumbnailWithLinkRenderer?.thumbnail?.thumbnails,
        88,
      ),
      thumbnail: image(v.thumbnail?.thumbnails),
      duration,
      views: text(v.shortViewCountText) || text(v.viewCountText),
      age: text(v.publishedTimeText),
      live,
      channelId: (v.ownerText ?? v.longBylineText ?? v.shortBylineText)?.runs?.[0]?.navigationEndpoint?.browseEndpoint?.browseId ?? null,
    }
  }
  // The first channel id in something of YouTube's (a lockup keeps its channel's in its avatar's tap).
  const channelIdIn = (thing) => stringify(thing ?? {}).match(/"browseId":"(UC[\w-]{22})"/)?.[1] ?? null
  // Whether the signed-in player is subscribed to a channel (`null`: not signed in, or not said). A search's
  // channel says it on its button; a channel's page keeps it in YouTube's entity store (`frameworkUpdates`),
  // in the subscription state its header's button names by key, or (YouTube's older button) on the button.
  const subscribedIn = (button) => {
    if (!signedIn()) return null
    return typeof button?.subscribed === 'boolean' ? button.subscribed : null
  }
  const subscribedOnPage = (data, header) => {
    if (!signedIn()) return null
    const told = stringify(header ?? {})
    const states = (data?.frameworkUpdates?.entityBatchUpdate?.mutations ?? [])
      .map((mutation) => ({ key: mutation.entityKey, state: mutation.payload?.subscriptionStateEntity }))
      .filter(({ key, state }) => state && typeof state.subscribed === 'boolean' && (told.includes(key) || told.includes(state.key)))
    if (states.length) return states[0].state.subscribed
    const older = told.match(/"subscribeButtonRenderer":\{[^{}]*"subscribed":(true|false)/)
    return older ? older[1] === 'true' : null
  }
  // A channel, as a search lists it (YouTube swaps two texts: with a handle, the count is in the "videos").
  const fromChannelRenderer = (c) => {
    const handleOrCount = text(c.subscriberCountText)
    const handle = handleOrCount.startsWith('@') ? handleOrCount : ''
    return {
      channel: c.channelId,
      name: text(c.title),
      avatar: image(c.thumbnail?.thumbnails, 176),
      banner: null,
      handle,
      subscribers: handle ? text(c.videoCountText) : handleOrCount,
      videos: '',
      subscribed: subscribedIn(c.subscriptionButton),
    }
  }
  // A channel from its page's header (or a search's card for it, which has the same header).
  const fromHeader = (header, id) => {
    const rows = header?.metadata?.contentMetadataViewModel?.metadataRows ?? []
    const parts = rows.flatMap((row) => row.metadataParts ?? []).map((part) => text(part.text))
    const handle = parts.find((part) => part.startsWith('@')) ?? ''
    const counts = parts.filter((part) => !part.startsWith('@'))
    return {
      channel: id,
      name: text(header?.title?.dynamicTextViewModel?.text),
      avatar: image(header?.image?.decoratedAvatarViewModel?.avatar?.avatarViewModel?.image?.sources, 176),
      banner: image(header?.banner?.imageBannerViewModel?.image?.sources, 1280),
      handle,
      subscribers: counts[0] ?? '',
      videos: counts[1] ?? '',
      subscribed: null,
    }
  }
  const channelsIn = (items) =>
    items
      .map((item) => {
        if (item.channelRenderer) return fromChannelRenderer(item.channelRenderer)
        const card = item.officialCardViewModel
        if (card?.header?.pageHeaderViewModel) return fromHeader(card.header.pageHeaderViewModel, channelIdIn(card))
        return null
      })
      .filter((channel) => channel?.channel && channel.name)
  // A grid of videos (a feed, a channel's videos) and the token for more.
  const gridOf = (grid) => ({
    items: grid.map((item) => item.richItemRenderer?.content).filter(Boolean),
    more: grid.map((item) => item.continuationItemRenderer?.continuationEndpoint?.continuationCommand?.token).find(Boolean) ?? null,
  })
  const tabGrid = (data) =>
    (data.contents?.twoColumnBrowseResultsRenderer?.tabs ?? [])
      .map((tab) => tab.tabRenderer?.content?.richGridRenderer?.contents)
      .find(Boolean) ?? []
  const continuationGrid = (data) =>
    (data.onResponseReceivedActions ?? []).flatMap((a) => a.appendContinuationItemsAction?.continuationItems ?? [])
  // An Up next video (YouTube's newer "lockup").
  const fromLockup = (l) => {
    if (l.contentType !== 'LOCKUP_CONTENT_TYPE_VIDEO') return null
    const meta = l.metadata?.lockupMetadataViewModel
    const rows = meta?.metadata?.contentMetadataViewModel?.metadataRows ?? []
    const badges = (l.contentImage?.thumbnailViewModel?.overlays ?? []).flatMap(
      (o) => o.thumbnailBottomOverlayViewModel?.badges ?? o.thumbnailOverlayBadgeViewModel?.thumbnailBadges ?? [],
    )
    const badge = badges.map((b) => b.thumbnailBadgeViewModel).find(Boolean)
    const live = badge?.badgeStyle === 'THUMBNAIL_OVERLAY_BADGE_STYLE_LIVE'
    const parts = rows[1]?.metadataParts ?? []
    return {
      video: l.contentId,
      title: text(meta?.title),
      channel: text(rows[0]?.metadataParts?.[0]?.text),
      avatar: image(meta?.image?.decoratedAvatarViewModel?.avatar?.avatarViewModel?.image?.sources, 88),
      thumbnail: image(l.contentImage?.thumbnailViewModel?.image?.sources),
      duration: live ? '' : (badge?.text ?? ''),
      views: text(parts[0]?.text),
      age: text(parts[1]?.text),
      live,
      channelId: channelIdIn(l),
    }
  }
  const videosIn = (items) =>
    items
      .map((item) => {
        const listed = item.videoRenderer ?? item.playlistVideoRenderer
        return listed ? fromVideoRenderer(listed) : item.lockupViewModel ? fromLockup(item.lockupViewModel) : null
      })
      .filter((video) => video?.video && video.title)
  // A playlist, as a search lists it: its cover (its first video's), title, channel and YouTube's count ("13
  // videos"). YouTube's mixes (`RD…`, endless and made up as they play) are left out.
  const fromPlaylistLockup = (l) => {
    if (l.contentType !== 'LOCKUP_CONTENT_TYPE_PLAYLIST' || !l.contentId || l.contentId.startsWith('RD')) return null
    const meta = l.metadata?.lockupMetadataViewModel
    const rows = meta?.metadata?.contentMetadataViewModel?.metadataRows ?? []
    const cover = l.contentImage?.collectionThumbnailViewModel?.primaryThumbnail?.thumbnailViewModel
    const badges = (cover?.overlays ?? []).flatMap((o) => o.thumbnailOverlayBadgeViewModel?.thumbnailBadges ?? [])
    return {
      playlist: l.contentId,
      title: text(meta?.title),
      channel: text(rows[0]?.metadataParts?.[0]?.text),
      channelId: channelIdIn(rows[0]),
      thumbnail: image(cover?.image?.sources),
      count: badges.map((b) => b.thumbnailBadgeViewModel?.text).find(Boolean) ?? '',
      views: '',
    }
  }
  // A search's videos and playlists, each playlist with where it sits among the videos (`after`: how many come
  // before it).
  const resultsIn = (items) => {
    const videos = []
    const playlists = []
    for (const item of items) {
      const [video] = videosIn([item])
      const playlist = !video && item.lockupViewModel ? fromPlaylistLockup(item.lockupViewModel) : null
      if (video) videos.push(video)
      else if (playlist?.title) playlists.push({ ...playlist, after: videos.length })
    }
    return { videos, playlists }
  }
  // A playlist from its page's header: its owner ("by ProMods"), and YouTube's counts ("182 videos", "1M views").
  const fromPlaylistHeader = (header, id) => {
    const rows = header?.metadata?.contentMetadataViewModel?.metadataRows ?? []
    const owner = rows[0]?.metadataParts?.[0]
    const counts = rows
      .slice(1)
      .flatMap((row) => row.metadataParts ?? [])
      .map((part) => text(part.text))
      .filter((part) => /\d/.test(part))
    return {
      playlist: id,
      title: text(header?.title?.dynamicTextViewModel?.text),
      channel: text(owner?.avatarStack?.avatarStackViewModel?.text ?? owner?.text),
      channelId: channelIdIn(owner),
      thumbnail: image(header?.heroImage?.contentPreviewImageViewModel?.image?.sources),
      count: counts[0] ?? '',
      views: counts[1] ?? '',
    }
  }
  // A playlist page's videos (in its sections), and the token for more: the list's own last item (YouTube's
  // older kind, or its newer view model; the page's own, for its sections after, comes later).
  const playlistItems = (data) =>
    (data.contents?.twoColumnBrowseResultsRenderer?.tabs ?? [])
      .flatMap((tab) => tab.tabRenderer?.content?.sectionListRenderer?.contents ?? [])
      .flatMap((section) => section.itemSectionRenderer?.contents ?? [section])
      .flatMap((item) => item.playlistVideoListRenderer?.contents ?? [item])
  const moreIn = (list) =>
    list
      .map((item) => item.continuationItemRenderer ?? item.continuationItemViewModel)
      .map((more) => more && stringify(more).match(/"continuationCommand":\{"token":"([^"]+)"/)?.[1])
      .find(Boolean) ?? null
  const answer = async (command) => {
    const reply = (fields) => window.chrome?.webview?.postMessage({ answer: command.id, ...fields })
    try {
      switch (command.action) {
        case 'search': {
          // YouTube's own filters: channels only, playlists only.
          const params = { channels: 'EgIQAg%3D%3D', playlists: 'EgIQAw%3D%3D' }[command.filter]
          const data = command.more
            ? await innertube('search', { continuation: command.more })
            : await innertube('search', params ? { query: command.query, params } : { query: command.query })
          const sections = command.more
            ? (data.onResponseReceivedCommands ?? []).flatMap((c) => c.appendContinuationItemsAction?.continuationItems ?? [])
            : (data.contents?.twoColumnSearchResultsRenderer?.primaryContents?.sectionListRenderer?.contents ?? [])
          const items = sections.flatMap((section) => section.itemSectionRenderer?.contents ?? [])
          const more = sections
            .map((section) => section.continuationItemRenderer?.continuationEndpoint?.continuationCommand?.token)
            .find(Boolean)
          reply({ ...resultsIn(items), channels: channelsIn(items), more: more ?? null })
          break
        }
        case 'playlist': {
          // A playlist's page: what it is and its videos (a hundred at a time), or its next videos.
          if (command.more) {
            const list = continuationGrid(await innertube('browse', { continuation: command.more }))
            reply({ videos: videosIn(list), more: moreIn(list) })
            break
          }
          const data = await innertube('browse', { browseId: `VL${command.playlist}` })
          const header = data.header?.pageHeaderRenderer?.content?.pageHeaderViewModel
          const list = playlistItems(data)
          const playlist = header ? fromPlaylistHeader(header, command.playlist) : null
          reply({ playlist, videos: videosIn(list), more: moreIn(list) })
          break
        }
        case 'channel': {
          // A channel's page: its header and its videos (the Videos tab), or its next videos.
          if (command.more) {
            const { items, more } = gridOf(continuationGrid(await innertube('browse', { continuation: command.more })))
            reply({ videos: videosIn(items), more })
            break
          }
          const data = await innertube('browse', { browseId: command.channel, params: 'EgZ2aWRlb3PyBgQKAjoA' })
          const header = data.header?.pageHeaderRenderer?.content?.pageHeaderViewModel
          const { items, more } = gridOf(tabGrid(data))
          const channel = header ? { ...fromHeader(header, command.channel), subscribed: subscribedOnPage(data, data.header) } : null
          reply({ channel, videos: videosIn(items), more })
          break
        }
        case 'subscribe': {
          // The signed-in player subscribes to a channel, or unsubscribes, as YouTube's own button does.
          await innertube(command.on ? 'subscription/subscribe' : 'subscription/unsubscribe', {
            channelIds: [command.channel],
          })
          reply({})
          break
        }
        case 'feed': {
          // The signed-in player's Home or Subscriptions: a grid of videos (shelves of Shorts and the like
          // left out), and the token for more.
          const data = command.more
            ? await innertube('browse', { continuation: command.more })
            : await innertube('browse', { browseId: command.which === 'subscriptions' ? 'FEsubscriptions' : 'FEwhat_to_watch' })
          const grid = command.more
            ? (data.onResponseReceivedActions ?? []).flatMap((a) => a.appendContinuationItemsAction?.continuationItems ?? [])
            : ((data.contents?.twoColumnBrowseResultsRenderer?.tabs ?? [])
                .map((tab) => tab.tabRenderer?.content?.richGridRenderer?.contents)
                .find(Boolean) ?? [])
          const items = grid.map((item) => item.richItemRenderer?.content).filter(Boolean)
          const more = grid
            .map((item) => item.continuationItemRenderer?.continuationEndpoint?.continuationCommand?.token)
            .find(Boolean)
          reply({ videos: videosIn(items), more: more ?? null })
          break
        }
        case 'related': {
          const data = await innertube('next', { videoId: command.video })
          const results = data.contents?.twoColumnWatchNextResults?.secondaryResults?.secondaryResults?.results ?? []
          reply({ videos: videosIn(results.flatMap((r) => r.itemSectionRenderer?.contents ?? [r])) })
          break
        }
        case 'suggest': {
          const query = encodeURIComponent(command.query)
          const hl = language ? `&hl=${encodeURIComponent(language)}` : ''
          const response = await fetch(
            `https://suggestqueries-clients6.youtube.com/complete/search?client=youtube&ds=yt${hl}&q=${query}`,
            { credentials: 'include' },
          )
          // `window.google.ac.h([query, [[suggestion, …], …], …])`
          const body = await response.text()
          const list = parse(body.slice(body.indexOf('(') + 1, body.lastIndexOf(')')))
          reply({ suggestions: (list[1] ?? []).map((entry) => entry[0]).filter((s) => typeof s === 'string') })
          break
        }
      }
    } catch {
      reply({ failed: true })
    }
  }

  // Whether this page is signed in, and the picture for the player, told once it's loaded: nothing else of
  // the account (WestPlay neither asks nor keeps).
  let accountTold = false
  const tellAccount = async () => {
    accountTold = true
    let avatar = null
    if (signedIn()) {
      try {
        const menu = await innertube('account/account_menu', {})
        const header = (menu.actions ?? [])
          .flatMap((a) => a.openPopupAction?.popup?.multiPageMenuRenderer?.header ?? [])
          .map((h) => h.activeAccountHeaderRenderer)
          .find(Boolean)
        avatar = image(header?.accountPhoto?.thumbnails, 88)
      } catch {}
    }
    window.chrome?.webview?.postMessage({ account: { signedIn: signedIn(), avatar } })
  }

  // The captions and the picture's quality, as chosen in the app, kept to on every video. Captions: the track
  // picked, on the video it was picked on; on the videos after, its language's (its own captions before its
  // automatic ones), or none. Quality: YouTube's own choice (`auto`), or the best the video has at or under
  // the one chosen.
  const QUALITIES = ['tiny', 'small', 'medium', 'large', 'hd720', 'hd1080', 'hd1440', 'hd2160', 'hd2880', 'highres']
  let captionLanguage = null
  let picked = { video: null, track: null }
  let quality = 'auto'
  // A video's captions, as its player data lists them (`.en`; `a.en`: automatic).
  const captionTracks = (p) =>
    (p.getPlayerResponse?.()?.captions?.playerCaptionsTracklistRenderer?.captionTracks ?? [])
      .map((t) => ({ track: t.vssId, language: t.languageCode ?? '', auto: t.kind === 'asr', name: text(t.name) }))
      .filter((t) => t.track && t.name)
  const wantedTrack = (video, tracks) => {
    if (picked.video === video) return picked.track
    if (!captionLanguage) return null
    const base = (code) => code.split('-')[0]
    const same = tracks.filter((t) => t.language === captionLanguage)
    const near = tracks.filter((t) => t.language !== captionLanguage && base(t.language) === base(captionLanguage))
    const ranked = [...same, ...near]
    return (ranked.find((t) => !t.auto) ?? ranked[0])?.track ?? null
  }
  // The track shown made the one wanted: YouTube's captions module loaded for one (it then knows the video's
  // tracks), none shown for none. Tried again a while after, until it is.
  let captionsTried = { key: '', at: 0 }
  const keepCaptions = (p, video, wanted) => {
    const shown = p.getOption?.('captions', 'track')?.vss_id ?? null
    const key = `${video}:${wanted}`
    if (shown === wanted || (captionsTried.key === key && Date.now() - captionsTried.at < 2000)) return
    captionsTried = { key, at: Date.now() }
    if (!wanted) {
      p.setOption?.('captions', 'track', {})
      return
    }
    p.loadModule?.('captions')
    // The module lists automatic captions only when asked to include them.
    const track = (p.getOption?.('captions', 'tracklist', { includeAsr: true }) ?? []).find((t) => t.vss_id === wanted)
    if (track) p.setOption('captions', 'track', track)
  }
  // Once a video for a quality held to (YouTube keeps it while it plays), or once back to YouTube's choice.
  let qualityKept = ''
  let held = false
  const keepQuality = (p, video) => {
    const levels = (p.getAvailableQualityLevels?.() ?? []).filter((level) => QUALITIES.includes(level))
    if (!levels.length) return
    const most = QUALITIES.indexOf(quality)
    const target = quality === 'auto' ? 'auto' : (levels.find((level) => QUALITIES.indexOf(level) <= most) ?? levels.at(-1))
    const key = `${video}:${target}`
    if (qualityKept === key || (target === 'auto' && !held)) return
    qualityKept = key
    held = target !== 'auto'
    p.setPlaybackQualityRange(target, target)
  }

  // The playback speed, kept to on the videos after (as YouTube's app keeps it); the audio track picked for
  // the video playing (a dubbed one, or its original).
  let rate = 1
  const keepRate = (p) => {
    if (p.getPlaybackRate?.() !== rate && (p.getAvailablePlaybackRates?.() ?? []).includes(rate)) p.setPlaybackRate(rate)
  }
  // A video's audio tracks as its player has them: each one's id, and its name and whether it's the default
  // (in a field of the track's whose name YouTube's build changes: found by what it holds).
  const audioTracks = (p) =>
    (p.getAvailableAudioTracks?.() ?? [])
      .map((track) => {
        const about = Object.values(track).find((v) => v && typeof v === 'object' && typeof v.name === 'string' && 'isDefault' in v)
        return about && typeof track.id === 'string' ? { track, id: track.id, name: about.name, original: !!about.isDefault } : null
      })
      .filter(Boolean)

  // The truck's own speakers (cab_sound.js), playing the video's sound.
  const cab = window.westplayCabSound(() => document.querySelector('#movie_player video.html5-main-video'), (message) =>
    window.chrome?.webview?.postMessage(message),
  )

  // Commands that came before they could be carried out, in order.
  const waiting = []
  // No quality held to: YouTube picks it by the player's size (the screen's) and the connection, as in a
  // browser (held to 720p, a slow connection could only stall).
  const run = (command) => {
    if (command.action === 'language') {
      language = command.code
      return
    }
    if (command.action === 'crop') {
      document.documentElement.classList.toggle('westplay-crop', command.on)
      return
    }
    if (command.action === 'captions') {
      // A track picked (or none) for the video playing; its language for the videos after.
      const video = player()?.getVideoData?.()?.video_id ?? null
      const track = command.track ?? null
      const listed = player() && captionTracks(player()).find((t) => t.track === track)
      captionLanguage = track ? (listed?.language ?? track.replace(/^a?\./, '').split('.')[0]) : null
      picked = { video, track }
      return
    }
    if (command.action === 'quality') {
      quality = command.quality
      return
    }
    if (command.action === 'rate') {
      rate = command.rate
      return
    }
    if (command.action === 'stableVolume') {
      player()?.setDrcUserPreference?.(command.on)
      return
    }
    if (command.action === 'voiceBoost') {
      player()?.setVoiceBoostUserPreference?.(command.on)
      return
    }
    if (command.action === 'audioTrack') {
      const p = player()
      const picked = p && audioTracks(p).find((t) => t.id === command.id)
      if (picked) p.setAudioTrack(picked.track)
      return
    }
    if (command.action === 'speakers') {
      cab.speakers(command.speakers)
      return
    }
    if (command.action === 'outside') {
      cab.outside(command.on, command.distance)
      return
    }
    if (command.action === 'head') {
      cab.head(command)
      return
    }
    if (command.action === 'equalizer') {
      cab.equalizer(command)
      return
    }
    if (command.action === 'spectrum') {
      cab.spectrum(command.on)
      return
    }
    if (DATA.has(command.action)) {
      if (config()) answer(command)
      else waiting.push(command)
      return
    }
    const p = player()
    if (command.action === 'load') {
      if (p) {
        p.loadVideoById({ videoId: command.video, startSeconds: command.start ?? 0 })
        setTimeout(() => p.playVideo(), 300)
      } else if (app()) {
        watch(command.video, command.start)
      } else {
        waiting.push(command)
      }
      return
    }
    if (!p) {
      waiting.push(command)
      return
    }
    switch (command.action) {
      case 'play':
        p.playVideo()
        break
      case 'pause':
        p.pauseVideo()
        break
      case 'seek':
        p.seekTo(command.seconds, true)
        break
      case 'volume':
        p.setVolume(command.percent)
        if (command.percent > 0) p.unMute()
        break
    }
  }
  window.chrome?.webview?.addEventListener('message', (event) => run(event.data))
  // Here: the host sends what it held back for this page.
  window.chrome?.webview?.postMessage({ ready: true })

  // How it plays, for the host: a report when anything changes, and each second while playing.
  // Why a video hasn't started, for the host's log (2026-10-27: players' videos stayed loading, the log saying
  // nothing): YouTube's own say on whether it plays (its playability status and reason), the player's last error,
  // the way the request was written and how often the video was held back, and what the video element has (its
  // ready and network states, its error, the seconds buffered, the frames decoded). Nothing of the player's account.
  let playerError = null
  const listened = new WeakSet()
  const listen = (p) => {
    if (listened.has(p)) return
    listened.add(p)
    try {
      p.addEventListener('onError', (code) => (playerError = typeof code === 'number' ? code : (code?.data ?? null)))
    } catch {}
  }
  const diagnosis = (p) => {
    const playability = p.getPlayerResponse?.()?.playabilityStatus ?? {}
    const element = document.querySelector('#movie_player video')
    let buffered = 0
    try {
      for (let i = 0; i < (element?.buffered?.length ?? 0); i++) buffered += element.buffered.end(i) - element.buffered.start(i)
    } catch {}
    return {
      playability: typeof playability.status === 'string' ? playability.status : null,
      reason: typeof playability.reason === 'string' ? playability.reason.slice(0, 80) : null,
      error: playerError,
      way,
      ways: WAYS.length,
      holds,
      ready: element?.readyState ?? -1,
      network: element?.networkState ?? -1,
      mediaError: element?.error?.code ?? null,
      buffered: Math.round(buffered * 10) / 10,
      frames: element?.getVideoPlaybackQuality?.()?.totalVideoFrames ?? 0,
    }
  }

  // While it plays: the picture not over the whole page, said (null: it is), each video element in the player with
  // its box, then what around the player makes its fixed place another box's than the page's.
  const picture = () => {
    const videos = [...document.querySelectorAll('#movie_player video')]
    const box = (e) => {
      const r = e.getBoundingClientRect()
      return `${Math.round(r.x)},${Math.round(r.y)} ${Math.round(r.width)}x${Math.round(r.height)}`
    }
    const page = `${innerWidth}x${innerHeight}`
    const fills = (e) => {
      const r = e.getBoundingClientRect()
      return Math.abs(r.x) < 2 && Math.abs(r.y) < 2 && Math.abs(r.width - innerWidth) < 2 && Math.abs(r.height - innerHeight) < 2
    }
    const shown = videos.filter((v) => !v.paused && v.src)
    if (shown.length > 0 && shown.every(fills) && videos.length === 1) return null
    const held = []
    for (let e = document.getElementById('movie_player')?.parentElement; e && e !== document.documentElement; e = e.parentElement) {
      const c = getComputedStyle(e)
      if (c.transform !== 'none' || c.contain !== 'none' || c.filter !== 'none') held.push(`${e.tagName.toLowerCase()}#${e.id}`)
    }
    const each = videos.map((v) => `${v.className || 'no class'} ${v.paused ? 'paused' : 'playing'} at ${box(v)}`)
    return `${videos.length} video(s) in a ${page} page: ${each.join('; ')}${held.length ? `; held by ${held.join(', ')}` : ''}`
  }

  let last = ''
  let lastAt = 0
  // The last video YouTube's warning was got past for (asked for again once).
  let warned = ''
  // The way a video was asked for when YouTube said it unplayable (null: none waiting to be told it played).
  let plainAfter = null
  setInterval(() => {
    // What waited and can be carried out now (in order: what still can't waits on).
    for (const command of waiting.splice(0)) run(command)
    if (!accountTold && settled && config()) tellAccount()
    const p = player()
    if (!p) return
    listen(p)
    // The speakers' way, on the video there is now (YouTube may make a new one); the sound's context woken if
    // it sleeps.
    cab.tick()
    const state = p.getPlayerState()
    const data = p.getVideoData?.() ?? {}
    // Nothing to tell before the player has a video.
    if (!data.video_id) return
    if (state === 1) played = data.video_id
    // YouTube's warning standing in for the video (it came with YouTube's own move to it, asked before this
    // page could say continue): asked for again, once a video, this time saying so (the request's
    // contentCheckOk, above).
    if (p.getPlayerResponse?.()?.playabilityStatus?.status === 'CONTENT_CHECK_REQUIRED' && warned !== data.video_id) {
      warned = data.video_id
      p.loadVideoById({ videoId: data.video_id, startSeconds: 0 })
      setTimeout(() => p.playVideo(), 300)
      return
    }
    // "This content isn't available, try again later" for a request written a way: YouTube's answer to one it
    // takes for an ad blocker's (a player's, 2026-10-27: the third way kept, and every video after it so). Asked
    // for again once as YouTube writes it (it may wait for the ad, but plays). Playing, that way is left for the
    // videos after it; still unplayable, the video is unavailable whatever the way, and the ways stay.
    if (
      p.getPlayerResponse?.()?.playabilityStatus?.status === 'UNPLAYABLE' &&
      writtenFor === data.video_id &&
      plain !== data.video_id
    ) {
      plain = data.video_id
      plainAfter = way
      p.loadVideoById({ videoId: data.video_id, startSeconds: 0 })
      setTimeout(() => p.playVideo(), 300)
      return
    }
    if (state === 1 && plain === data.video_id && plainAfter !== null) {
      way = Math.max(way, plainAfter + 1)
      plainAfter = null
    }
    const tracks = captionTracks(p)
    // Once it has played (YouTube has the video by then; not while it's asked for again past a hold).
    if (played === data.video_id) {
      try {
        keepCaptions(p, data.video_id, wantedTrack(data.video_id, tracks))
        keepQuality(p, data.video_id)
        keepRate(p)
      } catch {}
    }
    // The captions YouTube shows (not those asked for: told as they show).
    let caption = null
    try {
      caption = p.getOption?.('captions', 'track')?.vss_id ?? null
    } catch {}
    const qualities = (p.getAvailableQualityData?.() ?? [])
      .filter((q) => q.isPlayable !== false && QUALITIES.includes(q.quality))
      .map((q) => ({ quality: q.quality, label: q.qualityLabel ?? '' }))
    // Its audio tracks (only with more than one: dubbed) and the one playing; Stable volume and Voice boost,
    // as the player has them (each null where the video doesn't offer it).
    let dubs = []
    let audio = null
    let stableVolume = null
    let voiceBoost = null
    try {
      dubs = audioTracks(p)
      audio = dubs.find((t) => t.id === p.getAudioTrack?.()?.id)?.id ?? null
      if (dubs.length < 2) dubs = []
      if (p.hasDrcAudioTrack?.()) stableVolume = !!p.getDrcUserPreference?.()
      if ((p.getVoiceBoostState?.() ?? 0) !== 0) voiceBoost = !!p.getVoiceBoostUserPreference?.()
    } catch {}
    const report = {
      video: data.video_id ?? null,
      title: data.title ?? '',
      author: data.author ?? '',
      live: !!data.isLive,
      // -1 unstarted, 0 ended, 1 playing, 2 paused, 3 buffering, 5 cued.
      state,
      time: Math.round((p.getCurrentTime?.() ?? 0) * 10) / 10,
      duration: Math.round(p.getDuration?.() ?? 0),
      volume: p.isMuted?.() ? 0 : (p.getVolume?.() ?? 100),
      ad: !!document.getElementById('movie_player')?.classList.contains('ad-showing'),
      // Its captions and the one shown; its qualities (best first), the one chosen and the one playing.
      captions: tracks.map(({ track, name }) => ({ track, name })),
      caption,
      qualities,
      quality,
      qualityLabel: p.getPlaybackQualityLabel?.() ?? '',
      rates: p.getAvailablePlaybackRates?.() ?? [],
      rate: p.getPlaybackRate?.() ?? 1,
      audioTracks: dubs.map(({ id, name, original }) => ({ id, name, original })),
      audioTrack: audio,
      stableVolume,
      voiceBoost,
      cabLevel: cab.level(),
      // Before it plays (unstarted, buffering, cued): why not yet.
      diag: state === -1 || state === 3 || state === 5 ? diagnosis(p) : null,
      // While it plays, the picture not over the page (for the log).
      picture: state === 1 ? picture() : null,
    }
    // Anything but the time changed, or a second has passed while it plays (the page moves its progress
    // between reports), or five while it hasn't started (the host's log hears how it's doing).
    const text = stringify({ ...report, time: undefined, cabLevel: undefined, diag: undefined })
    const now = Date.now()
    if (text !== last || (state === 1 && now - lastAt >= 1000) || (report.diag && now - lastAt >= 5000)) {
      last = text
      lastAt = now
      window.chrome?.webview?.postMessage(report)
    }
  }, 250)
})()
