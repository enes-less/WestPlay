// WestPlay, in Apple Music's web player (apple_music.rs): Apple's own page, run hidden under the cab. The Music
// app the player sees is WestPlay's own; this answers its questions and plays as it asks, through the page's
// own player (MusicKit, which the page runs and configures itself):
//
// - Questions (Home, New, Radio, the Library, a search, an album's, playlist's or artist's page) are asked of
//   Apple Music's API through MusicKit, with the page's own tokens (which never leave it).
// - Playing is MusicKit's queue, as the page's own buttons set it; what plays is MusicKit's, as it says it.
// - Signing in is Apple's own dialog, in a window on the desktop (apple_music_sign_in.js); this page then
//   loads again, signed in.
//
// Nothing of the account leaves the page: its tokens, its storefront and subscription stay here. The answers
// are what the app shows (names, covers, what plays); of the account, only whether it's signed in and its
// Apple Music profile's picture (to show on the account's button), never its name.
;(() => {
  if (location.hostname !== 'music.apple.com' || window.top !== window) return
  const post = (message) => window.chrome?.webview?.postMessage(message)

  // --- How long playing takes, step by step: for the host's log, the kinds of steps and their times only
  // (Apple's requests by kind, the audio element's and MusicKit's own events), never addresses or contents.
  const steps = { since: null, list: [], media: [] }
  const step = (what) => {
    if (steps.since !== null && steps.list.length < 60) {
      steps.list.push(`${what} ${Math.round(performance.now() - steps.since)}`)
    }
  }
  const REQUESTS = [
    [/licen[sc]e|keyserver|widevine|\bfps\b/i, 'licence'],
    [/webPlayback/i, 'asset'],
    [/\.m3u8/i, 'playlist'],
    [/\.(mp4|m4a|aac|ts)(\?|$)/i, 'segment'],
  ]
  const kindOf = (url) => REQUESTS.find(([pattern]) => pattern.test(String(url)))?.[1] ?? null
  const pageFetch = window.fetch.bind(window)

  // --- The next song, prepared --------------------------------------------------------------------------------
  //
  // Each song of Apple Music's starts with three requests in turn: its stream's addresses (`webPlayback`),
  // its stream (a playlist, then its audio by byte ranges), and its licence (for the browser's DRM, made at
  // the moment it plays). While a song plays, the next one's first two are made here, as Apple's player
  // makes them: its addresses kept to answer its player with at once, its stream's start fetched into the
  // browser's own cache (Apple marks it cacheable for a year). Only the licence is left to its moment.
  // Anything unexpected leaves Apple's player to its own requests.
  const ADDRESSES = /\/WebObjects\/MZPlay\.woa\/wa\/webPlayback$/
  const prepared = {
    // Apple's own addresses request, copied (its headers, the page's own): asked again with another song.
    template: null,
    // Addresses by song (Apple's catalogue id), with when they came: kept a while (Back finds them too).
    answers: new Map(),
    // The stream quality Apple's player chose, to fetch the next one's alike.
    quality: null,
    // The song being prepared, or done.
    song: null,
    // What preparing did (said with the next play's steps): its own steps and times.
    log: [],
  }
  const prepLog = (what) => {
    if (prepared.log.length < 12) prepared.log.push(`prep:${what} ${Math.round(performance.now() % 100000)}`)
  }
  const KEEP_ANSWERS = 10 * 60 * 1000
  // How long into a song the next one is prepared.
  const PREPARE_AFTER = 1500
  const fresh = (song) => {
    const kept = prepared.answers.get(song)
    return kept && performance.now() - kept.at < KEEP_ANSWERS ? kept.text : null
  }
  const keep = (song, text) => {
    prepared.answers.set(song, { text, at: performance.now() })
    for (const [each, { at }] of prepared.answers) {
      if (performance.now() - at >= KEEP_ANSWERS) prepared.answers.delete(each)
    }
  }
  const songOf = (text) => {
    try {
      const song = JSON.parse(text)?.salableAdamId
      return /^\d+$/.test(String(song ?? '')) ? String(song) : null
    } catch {
      return null
    }
  }
  const addressesAnswer = (text) => new Response(text, { status: 200, headers: { 'content-type': 'application/json' } })

  // Apple's player asking a song's addresses: answered with what's kept for it, or asked of Apple (and kept).
  const addresses = async (input, init) => {
    let copy = null
    try {
      copy = input.clone()
      if (!prepared.template) prepared.template = input.clone()
    } catch {
      return pageFetch(input, init)
    }
    const song = songOf(await copy.text().catch(() => ''))
    const known = song ? fresh(song) : null
    if (known) {
      step('asset:prepared')
      return addressesAnswer(known)
    }
    const asked = pageFetch(input, init)
    if (song) {
      asked
        .then((response) => (response.ok ? response.clone().text() : null))
        .then((text) => text && keep(song, text))
        .catch(() => {})
    }
    return asked
  }

  // The stream quality its player took: the end of its stream's file name (`.rphq.aac.wa.m3u8`), which every
  // song's stream of that quality ends with.
  const qualityOf = (url) => {
    const name = String(url).split('?')[0].split('/').pop() ?? ''
    const dot = name.indexOf('.')
    return dot > 0 ? name.slice(dot) : null
  }
  const tookStream = (url) => {
    if (/aod/.test(url)) prepared.quality = qualityOf(url) ?? prepared.quality
  }

  // The start of the next song's stream, kept here (the browser's cache doesn't keep byte ranges the way Apple's
  // player asks for them): its playlist, its init and first audio, by the same byte ranges Apple's player asks
  // for, each handed to it once as it asks. Fetched together; a few hundred kilobytes.
  const streams = new Map()
  const KEEP_STREAMS = 12
  const streamKey = (url, range) => `${url}|${range ?? ''}`
  const keepStream = (key, kept) => {
    streams.set(key, kept)
    while (streams.size > KEEP_STREAMS) streams.delete(streams.keys().next().value)
  }
  const warmStream = async (url) => {
    const response = await pageFetch(url)
    if (!response.ok) return
    const playlist = await response.text()
    keepStream(streamKey(url), {
      body: playlist,
      status: 200,
      headers: { 'content-type': response.headers.get('content-type') ?? 'application/vnd.apple.mpegurl' },
    })
    const lines = playlist.split('\n')
    const ranges = []
    const init = /#EXT-X-MAP:URI="([^"]+)",BYTERANGE="(\d+)@(\d+)"/.exec(playlist)
    if (init) ranges.push([init[1], Number(init[2]), Number(init[3])])
    for (let i = 0; i < lines.length && ranges.length < 3; i++) {
      const range = /^#EXT-X-BYTERANGE:(\d+)@(\d+)/.exec(lines[i])
      const uri = lines[i + 1]?.trim()
      if (range && uri && !uri.startsWith('#')) ranges.push([uri, Number(range[1]), Number(range[2])])
    }
    await Promise.all(
      ranges.map(async ([uri, length, offset]) => {
        const address = new URL(uri, url).href
        const range = `bytes=${offset}-${offset + length - 1}`
        const piece = await pageFetch(address, { headers: { range } }).catch(() => null)
        if (!piece || (piece.status !== 206 && piece.status !== 200)) return
        const headers = {}
        for (const name of ['content-type', 'content-range', 'content-length']) {
          const value = piece.headers.get(name)
          if (value) headers[name] = value
        }
        keepStream(streamKey(address, range), { body: await piece.arrayBuffer(), status: piece.status, headers })
      }),
    )
  }
  // A piece of a stream Apple's player asks for, if it was kept: handed over once.
  const keptStream = (url, init) => {
    if (streams.size === 0) return null
    let range = null
    try {
      range = new Headers(init?.headers ?? {}).get('range')
    } catch {
      return null
    }
    const key = streamKey(url, range)
    const kept = streams.get(key)
    if (!kept) return null
    streams.delete(key)
    return new Response(kept.body, { status: kept.status, headers: kept.headers })
  }

  // The song after the one playing, by Apple Music's catalogue id (a Library song's own, its catalogue one).
  const nextSong = (music) => {
    const item = music.queue?.items?.[(music.nowPlayingItemIndex ?? -1) + 1]
    const id = item?.attributes?.playParams?.catalogId ?? item?.playParams?.catalogId ?? item?.id
    return /^\d+$/.test(String(id ?? '')) ? String(id) : null
  }

  // Prepares the next song (once each): its addresses, then its stream's start, at the quality chosen.
  const prepareNext = async (music) => {
    const song = nextSong(music)
    if (!song || song === prepared.song || !prepared.template || !music.isAuthorized) {
      prepLog(!song ? 'nonext' : song === prepared.song ? 'done' : !prepared.template ? 'notemplate' : 'signedout')
      return
    }
    prepared.song = song
    prepLog('start')
    try {
      let text = fresh(song)
      if (!text) {
        const request = new Request(prepared.template, { body: JSON.stringify({ salableAdamId: song }) })
        const response = await pageFetch(request)
        if (!response.ok) throw new Error(`addresses: ${response.status}`)
        text = await response.text()
        keep(song, text)
      }
      const assets = JSON.parse(text)?.songList?.[0]?.assets ?? []
      prepLog('addresses')
      const stream = assets.find((each) => prepared.quality && qualityOf(each?.URL) === prepared.quality) ?? null
      if (!stream?.URL) prepLog(prepared.quality ? 'noquality' : 'qualityunknown')
      if (stream?.URL) {
        await warmStream(stream.URL)
        prepLog('stream')
      }
    } catch {
      // Not prepared: Apple's player asks for it itself, as ever.
      prepLog('failed')
      prepared.song = null
    }
  }

  window.fetch = function (input, init) {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : (input?.url ?? '')
    const kind = steps.since === null ? null : kindOf(url)
    if (input instanceof Request && input.method === 'POST' && ADDRESSES.test(new URL(url, location.href).pathname)) {
      if (kind) step(`${kind}>`)
      const asked = addresses(input, init)
      if (kind) asked.then(() => step(kind)).catch(() => step(`${kind}!`))
      return asked
    }
    if (/\.m3u8(\?|$)/.test(url)) tookStream(url)
    // The next song's stream start, prepared: handed over at once.
    const kept = typeof input === 'string' && (!init?.method || init.method === 'GET') ? keptStream(url, init) : null
    if (kept) {
      if (kind) step(`${kind}:prepared`)
      return Promise.resolve(kept)
    }
    const asked = pageFetch(input, init)
    if (kind) {
      step(`${kind}>`)
      asked.then(
        (response) => {
          // From the browser's cache: nothing came over the network.
          const timing = performance.getEntriesByName(response.url || url).at(-1)
          const cached = timing && timing.transferSize === 0 && timing.decodedBodySize > 0
          step(response.ok ? `${kind}${cached ? ':cache' : ''}` : `${kind}!${response.status}`)
        },
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
        (event) =>
          step(
            this.status >= 200 && this.status < 400
              ? kind
              : `${kind}!x${this.status}${event.type === 'loadend' && this.status === 0 ? 'aborted' : ''}`,
          ),
        { once: true },
      )
      this.addEventListener('abort', () => step(`${kind}!abort`), { once: true })
    }
    return pageOpen.call(this, method, url, ...rest)
  }
  const MEDIA_EVENTS = [
    'loadstart',
    'encrypted',
    'waitingforkey',
    'loadedmetadata',
    'canplay',
    'playing',
    'waiting',
    'stalled',
  ]
  const startSteps = () => {
    steps.since = performance.now()
    // What preparing did before, then this play's own steps.
    steps.list = prepared.log.splice(0)
    for (const element of document.querySelectorAll('audio, video')) {
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

  // --- MusicKit, as the page configures it -------------------------------------------------------------------

  const instance = () => {
    try {
      return window.MusicKit?.getInstance?.() ?? null
    } catch {
      return null
    }
  }
  const ready = new Promise((resolve) => {
    const look = () => {
      const music = instance()
      if (music?.storefrontId) resolve(music)
      else setTimeout(look, 200)
    }
    look()
  })

  // A request of Apple Music's API, with the page's tokens (MusicKit adds them): a question, or a change
  // (`method`, and what it sends).
  const api = async (path, parameters = {}, method = null, body = null) => {
    const music = await ready
    const fetchOptions = method ? { method, ...(body ? { body: JSON.stringify(body) } : {}) } : null
    const response = await music.api.music(path, parameters, fetchOptions ? { fetchOptions } : undefined)
    return response?.data ?? null
  }
  const catalog = async (path) => `/v1/catalog/${(await ready).storefrontId}${path}`

  // --- What the app shows, from Apple Music's answers ------------------------------------------------------

  const KINDS = {
    albums: 'album',
    'library-albums': 'album',
    playlists: 'playlist',
    'library-playlists': 'playlist',
    artists: 'artist',
    'library-artists': 'artist',
    stations: 'station',
    songs: 'song',
    'library-songs': 'song',
    'apple-curators': 'curator',
    curators: 'curator',
    'library-playlist-folders': 'folder',
  }

  // An artwork's address at `size` (Apple's addresses leave the size and format to fill in).
  const image = (artwork, size = 600, height = size) =>
    artwork?.url
      ? artwork.url
          .replace('{w}', String(size))
          .replace('{h}', String(height))
          .replace('{c}', 'bb')
          .replace('{f}', 'jpg')
      : null
  const color = (artwork) => (/^[0-9a-f]{6}$/i.test(artwork?.bgColor ?? '') ? `#${artwork.bgColor}` : '#2c2c2e')
  // A wide picture of it, as Apple's editors made one for New's and Radio's big cards (12:5; their other
  // wide ones are banners, with words that a card would cut).
  const wideOf = (attributes) => {
    const found = attributes?.editorialArtwork?.superHeroWide
    return found?.url ? image(found, 1200, 500) : null
  }
  const names = new Map()

  const item = (resource) => {
    const kind = KINDS[resource?.type]
    const attributes = resource?.attributes
    if (!kind || !attributes) return null
    const id = `${resource.type}:${resource.id}`
    const artwork = attributes.artwork ?? attributes.editorialArtwork?.bannerUber ?? null
    const name = attributes.name ?? attributes.title?.stringForDisplay ?? ''
    names.set(id, name)
    const by =
      kind === 'playlist'
        ? (attributes.curatorName ?? '')
        : kind === 'station'
          ? (attributes.editorialNotes?.tagline ?? attributes.plainEditorialNotes?.tagline ?? '')
          : (attributes.artistName ?? '')
    return {
      id,
      kind,
      name,
      by,
      note: attributes.plainEditorialNotes?.short ?? attributes.editorialNotes?.short ?? '',
      image: image(artwork),
      wide: wideOf(attributes),
      color: color(artwork),
      explicit: attributes.contentRating === 'explicit',
      live: Boolean(attributes.isLive),
    }
  }

  // A song's album, from the song (its own relationship, or the address of its page).
  const albumOf = (resource) => {
    const album = resource?.relationships?.albums?.data?.[0]
    if (album?.id) return `${album.type ?? 'albums'}:${album.id}`
    const url = resource?.attributes?.url ?? ''
    const found = url.match(/\/album\/[^/]+\/(\d+)/)
    return found ? `albums:${found[1]}` : null
  }
  const artistOf = (resource) => {
    const artist = resource?.relationships?.artists?.data?.[0]
    if (artist?.id) return `${artist.type ?? 'artists'}:${artist.id}`
    const url = resource?.attributes?.artistUrl ?? ''
    const found = url.match(/\/artist\/[^/]+\/(\d+)/)
    return found ? `artists:${found[1]}` : null
  }

  const track = (resource) => {
    if (!['songs', 'library-songs'].includes(resource?.type) || !resource.attributes) return null
    const attributes = resource.attributes
    return {
      id: `${resource.type}:${resource.id}`,
      name: attributes.name ?? '',
      artist: attributes.artistName ?? '',
      album: attributes.albumName ?? '',
      albumId: albumOf(resource),
      artistId: artistOf(resource),
      image: image(attributes.artwork),
      color: color(attributes.artwork),
      duration: Number(attributes.durationInMillis ?? 0) / 1000,
      explicit: attributes.contentRating === 'explicit',
      number: attributes.trackNumber ?? null,
    }
  }

  const items = (list) => (list ?? []).map(item).filter(Boolean)
  const tracks = (list) => (list ?? []).map(track).filter(Boolean)

  // A section of an editors' page (New, Radio, a category): its name, and its cards (a carousel's slides
  // each hold one) or its songs.
  const section = (element) => {
    const attributes = element?.attributes ?? {}
    const children = element?.relationships?.children?.data ?? []
    const contents = element?.relationships?.contents?.data ?? []
    // A carousel: each slide holds its card, with the slide's own label ("UPDATED PLAYLIST") if any.
    if (children.length > 0 && contents.length === 0) {
      const cards = children
        .map((child) => {
          const card = item(child?.relationships?.contents?.data?.[0])
          if (!card) return null
          const label = child.attributes?.designBadge ?? child.attributes?.designTag ?? ''
          return { ...card, label: typeof label === 'string' ? label : '' }
        })
        .filter(Boolean)
      return cards.length ? { title: attributes.name ?? '', style: 'hero', items: cards, songs: [] } : null
    }
    const songs = tracks(contents)
    if (songs.length > 0 && songs.length >= contents.length / 2) {
      return { title: attributes.name ?? '', style: 'songs', items: [], songs }
    }
    const cards = items(contents)
    return cards.length ? { title: attributes.name ?? '', style: 'shelf', items: cards, songs: [] } : null
  }

  // An editors' page (a grouping): its sections, in order.
  const grouping = (found) => {
    const top = found?.relationships?.tabs?.data?.[0] ?? found
    const elements = top?.relationships?.children?.data ?? []
    return { sections: elements.map(section).filter(Boolean) }
  }
  const GROUPING = {
    platform: 'web',
    'art[url]': 'c,f',
    extend: 'editorialArtwork,plainEditorialNotes,artistUrl',
    'include[albums]': 'artists',
    'include[songs]': 'artists,albums',
    'include[stations]': 'events',
  }
  const editorial = async (name) => {
    const music = await ready
    const found = await api(`/v1/editorial/${music.storefrontId}/groupings`, { ...GROUPING, name })
    return grouping(found?.data?.[0])
  }

  // Home, signed in: what was played lately, then Apple Music's picks for the account.
  const home = async () => {
    const [played, picks] = await Promise.all([
      api('/v1/me/recent/played', { limit: 10, 'art[url]': 'c,f' }).catch(() => null),
      api('/v1/me/recommendations', {
        timezone: (() => {
          const offset = -new Date().getTimezoneOffset()
          const hours = String(Math.floor(Math.abs(offset) / 60)).padStart(2, '0')
          return `${offset < 0 ? '-' : '+'}${hours}:${String(Math.abs(offset) % 60).padStart(2, '0')}`
        })(),
        'art[url]': 'c,f',
        extend: 'editorialArtwork,plainEditorialNotes',
      }),
    ])
    const sections = []
    const recent = items(played?.data)
    if (recent.length) sections.push({ title: '', style: 'recent', items: recent, songs: [] })
    for (const pick of picks?.data ?? []) {
      const contents = pick?.relationships?.contents?.data ?? []
      const cards = items(contents)
      if (cards.length === 0) continue
      sections.push({
        title: pick.attributes?.title?.stringForDisplay ?? '',
        style: 'shelf',
        items: cards,
        songs: [],
      })
    }
    return { sections }
  }

  // Search's start: Apple Music's categories.
  const browse = async () => {
    const music = await ready
    const found = await api(`/v1/recommendations/${music.storefrontId}`, {
      name: 'search-landing',
      platform: 'web',
      'art[url]': 'f',
      extend: 'editorialArtwork',
    })
    const all = (found?.data ?? []).flatMap((shelf) => shelf?.relationships?.contents?.data ?? [])
    return all
      .map((resource) => {
        const card = item(resource)
        if (!card) return null
        const art = resource.attributes?.editorialArtwork ?? {}
        const tile = art.bannerUber ?? art.brandLogo ?? Object.values(art).find((each) => each?.url)
        return { ...card, image: tile ? image(tile, 800, 400) : card.image, color: color(tile ?? null) }
      })
      .filter(Boolean)
  }

  // A category's page (Apple's curator for it).
  const curator = async ({ id }) => {
    const [type, key] = split(id)
    const found = await api(await catalog(`/${type}/${key}`), {
      platform: 'web',
      include: 'grouping',
      extend: 'editorialArtwork',
      'art[url]': 'c,f',
    })
    const page = found?.data?.[0]
    const group = page?.relationships?.grouping?.data?.[0]
    if (!group) return { sections: [] }
    const full = group.relationships?.tabs
      ? group
      : (await api(await catalog(`/groupings/${group.id}`), GROUPING).catch(() => null))?.data?.[0]
    return grouping(full ?? group)
  }

  const split = (id) => {
    const at = String(id ?? '').indexOf(':')
    if (at < 1) throw new Error('not an id')
    return [id.slice(0, at), id.slice(at + 1)]
  }

  const search = async ({ query }) => {
    const found = await api(await catalog('/search'), {
      term: query,
      types: 'songs,albums,artists,playlists,stations',
      limit: 12,
      with: 'topResults',
      'art[url]': 'c,f',
      'include[songs]': 'albums,artists',
    })
    const results = found?.results ?? {}
    return {
      top: items(results.topResults?.data).slice(0, 6),
      songs: tracks(results.songs?.data),
      artists: items(results.artists?.data),
      albums: items(results.albums?.data),
      playlists: items(results.playlists?.data),
      stations: items(results.stations?.data),
    }
  }

  // Every page of a list (a long playlist's songs), up to `most`.
  const allOf = async (first, most = 300) => {
    const found = [...(first?.data ?? [])]
    let next = first?.next
    while (next && found.length < most) {
      const page = await api(next).catch(() => null)
      if (!page) break
      found.push(...(page.data ?? []))
      next = page.next
    }
    return found
  }

  const collection = async ({ id }) => {
    const [type, key] = split(id)
    const library = type.startsWith('library-')
    const path = library ? `/v1/me/library/${type.slice(8)}/${key}` : await catalog(`/${type}/${key}`)
    const found = await api(path, {
      include: 'tracks',
      'include[songs]': 'albums,artists',
      extend: 'editorialArtwork,plainEditorialNotes,artistUrl',
      'art[url]': 'c,f',
    })
    const page = found?.data?.[0]
    const card = item(page)
    if (!card) throw new Error('not found')
    const songs = tracks(await allOf(page.relationships?.tracks))
    const attributes = page.attributes ?? {}
    const year = Number(String(attributes.releaseDate ?? '').slice(0, 4)) || null
    return {
      item: card,
      description: attributes.description?.standard ?? attributes.editorialNotes?.standard ?? '',
      genre: (attributes.genreNames ?? []).find((name) => name !== 'Music') ?? '',
      year,
      tracks: songs,
    }
  }

  const artist = async ({ id }) => {
    let [type, key] = split(id)
    // An artist of the Library: Apple Music's own page for them.
    if (type === 'library-artists') {
      const found = await api(`/v1/me/library/artists/${key}`, { include: 'catalog' })
      const own = found?.data?.[0]?.relationships?.catalog?.data?.[0]
      if (!own) throw new Error('no catalog artist')
      ;[type, key] = ['artists', own.id]
    }
    const found = await api(await catalog(`/${type}/${key}`), {
      views: 'top-songs,latest-release,full-albums,singles,featured-playlists,similar-artists',
      extend: 'editorialArtwork,artistBio',
      'art[url]': 'c,f',
      'include[songs]': 'albums',
    })
    const page = found?.data?.[0]
    const card = item(page)
    if (!card) throw new Error('not found')
    const views = page.views ?? {}
    const shelf = (name) => ({
      title: views[name]?.attributes?.title ?? '',
      style: 'shelf',
      items: items(views[name]?.data),
      songs: [],
    })
    return {
      // Their picture, large: the page's top.
      item: { ...card, wide: image(page.attributes?.artwork, 1400) },
      top: tracks(views['top-songs']?.data).slice(0, 10),
      sections: ['latest-release', 'full-albums', 'singles', 'featured-playlists', 'similar-artists']
        .map(shelf)
        .filter((each) => each.items.length > 0),
    }
  }

  // The Library's lists, as Apple's own web player asks for them. Albums, artists and songs: sorted by Apple
  // Music itself, which says the sorts it has, in the cab's language (`meta: 'sorts'`); playlists (and their
  // folders): the iPhone's five, done here (Apple's web data has no time a playlist was played: Last Played
  // goes by Apple Music's Recently Played of the Library's playlists, the rest after by date added).
  const SHELVES = {
    albums: ['/v1/me/library/albums', { include: 'catalog', 'include[library-albums]': 'artists' }],
    artists: ['/v1/me/library/artists', { include: 'catalog' }],
    songs: ['/v1/me/library/songs', { include: 'albums,catalog' }],
  }
  const RATED = { playlists: 'library-playlists', albums: 'library-albums', songs: 'library-songs' }
  const LANGUAGES = { tr: 'tr-TR', en: 'en-GB' }
  const PLAYLIST_SORTS = ['name', '-dateAdded', '-lastPlayed', '-updated', 'type']

  // The Library's playlists played lately, newest first (Apple Music's Recently Played of them, private ones
  // too; ten at a time, a hundred at most): each one's place, by its id.
  const playedOrder = async () => {
    const pages = await Promise.all(
      [0, 10, 20, 30, 40, 50, 60, 70, 80, 90].map((offset) =>
        api('/v1/me/recent/played', { limit: 10, offset, types: 'library-playlists' }).catch(() => null),
      ),
    )
    const order = new Map()
    for (const resource of pages.flatMap((page) => page?.data ?? [])) {
      if (resource?.id && !order.has(resource.id)) order.set(resource.id, order.size)
    }
    return order
  }

  // Which of the Library's things are favourites (Apple Music's rating of 1), asked a hundred at a time.
  const favouritesOf = async (type, ids) => {
    const loved = new Set()
    for (let at = 0; at < ids.length; at += 100) {
      const found = await api(`/v1/me/ratings/${type}`, { ids: ids.slice(at, at + 100).join(',') }).catch(() => null)
      for (const rating of found?.data ?? []) if (rating?.attributes?.value === 1) loved.add(rating.id)
    }
    return loved
  }

  // An artist or album of the Library has no picture of its own: its Apple Music one, if any.
  const withPicture = (resource) => {
    const own = resource?.relationships?.catalog?.data?.[0]
    return !resource?.attributes?.artwork && own?.attributes?.artwork
      ? { ...resource, attributes: { ...resource.attributes, artwork: own.attributes.artwork } }
      : resource
  }

  // Every page of a list, asked again with its own parameters and the next offset (Apple's own `next` may
  // leave the sort out), up to `most`; the first page's answer too (its `meta`).
  const everyPage = async (path, parameters, most = 500) => {
    const first = await api(path, parameters)
    const found = [...(first?.data ?? [])]
    let more = Boolean(first?.next)
    while (more && found.length < most) {
      const page = await api(path, { ...parameters, offset: found.length }).catch(() => null)
      const data = page?.data ?? []
      if (data.length === 0) break
      found.push(...data)
      more = Boolean(page.next)
    }
    return { first, list: found }
  }

  const library = async ({ shelf, sort, folder, language }) => {
    const l = LANGUAGES[language] ?? undefined
    if (shelf === 'recent') {
      const found = await api('/v1/me/library/recently-added', { limit: 25, 'art[url]': 'c,f' })
      return { items: items((found?.data ?? []).map(withPicture)), songs: [], sorts: [], sort: null }
    }
    if (shelf === 'playlists') {
      const where = folder ? split(folder)[1] : 'p.playlistsroot'
      const { list: found } = await everyPage(`/v1/me/library/playlist-folders/${where}/children`, {
        platform: 'web',
        include: 'catalog',
        limit: 100,
        'art[url]': 'c,f',
        ...(l ? { l } : {}),
      })
      const list = found.map(withPicture)
      const chosen = PLAYLIST_SORTS.includes(sort) ? sort : '-lastPlayed'
      // Playlist Type: folders, then the player's own playlists, then the rest; each by name.
      const rank = (resource) =>
        resource.type === 'library-playlist-folders' ? 0 : resource.attributes?.canEdit ? 1 : 2
      const name = (resource) => String(resource.attributes?.name ?? '')
      const added = (resource) => String(resource.attributes?.dateAdded ?? '')
      const updated = (resource) =>
        String(resource.attributes?.lastModifiedDate ?? resource.attributes?.dateAdded ?? '')
      const played = chosen === '-lastPlayed' ? await playedOrder().catch(() => new Map()) : new Map()
      const playedAt = (resource) => played.get(resource.id) ?? Infinity
      list.sort((a, b) =>
        chosen === 'name'
          ? name(a).localeCompare(name(b))
          : chosen === '-dateAdded'
            ? added(b).localeCompare(added(a))
            : chosen === '-updated'
              ? updated(b).localeCompare(updated(a))
              : chosen === '-lastPlayed'
                ? playedAt(a) - playedAt(b) || added(b).localeCompare(added(a))
                : rank(a) - rank(b) || name(a).localeCompare(name(b)),
      )
      const loved = await favouritesOf(
        RATED.playlists,
        list.filter((each) => each.type === 'library-playlists').map((each) => each.id),
      )
      return {
        items: list
          .map((resource) => {
            const card = item(resource)
            return card && { ...card, favorite: loved.has(resource.id) }
          })
          .filter(Boolean),
        songs: [],
        sorts: PLAYLIST_SORTS.map((id) => ({ id, label: '' })),
        sort: chosen,
      }
    }
    const [path, parameters] = SHELVES[shelf] ?? SHELVES.albums
    const { first: found, list: all } = await everyPage(path, {
      limit: 100,
      'art[url]': 'c,f',
      meta: 'sorts',
      ...parameters,
      ...(sort ? { sort } : {}),
      ...(l ? { l } : {}),
    })
    const sorts = (found?.meta?.sorts ?? [])
      .filter((each) => each?.name && each?.displayName)
      .map((each) => ({ id: String(each.name), label: String(each.displayName) }))
    const current = found?.meta?.currentSort ?? sort ?? null
    const list = all.map(withPicture)
    const loved = RATED[shelf]
      ? await favouritesOf(
          RATED[shelf],
          list.map((each) => each.id),
        )
      : new Set()
    if (shelf === 'songs') {
      return {
        items: [],
        songs: tracks(list).map((song) => ({ ...song, favorite: loved.has(split(song.id)[1]) })),
        sorts,
        sort: current,
      }
    }
    return {
      items: list
        .map((resource) => {
          const card = item(resource)
          return card && (RATED[shelf] ? { ...card, favorite: loved.has(resource.id) } : card)
        })
        .filter(Boolean),
      songs: [],
      sorts,
      sort: current,
    }
  }

  // A song's words: Apple Music's own (TTML), line by line with when each is sung, its stanzas, its songwriters.
  // A song of the Library's by its Apple Music one; none (an empty answer) without a subscription or words.
  const seconds = (time) => {
    const parts = String(time ?? '')
      .split(':')
      .map(Number)
    return parts.every((part) => Number.isFinite(part)) ? parts.reduce((total, part) => total * 60 + part, 0) : 0
  }
  const lyrics = async ({ id }) => {
    const [type, key] = split(id)
    let song = type === 'songs' ? key : null
    if (type === 'library-songs') {
      const found = await api(`/v1/me/library/songs/${key}`, {})
      const catalogId = found?.data?.[0]?.attributes?.playParams?.catalogId
      song = /^\d+$/.test(String(catalogId ?? '')) ? String(catalogId) : null
    }
    if (!song) return { synced: false, lines: [] }
    const found = await api(await catalog(`/songs/${song}/lyrics`), {}).catch(() => null)
    const ttml = found?.data?.[0]?.attributes?.ttml
    if (!ttml) return { synced: false, lines: [] }
    const doc = new DOMParser().parseFromString(ttml, 'application/xml')
    const root = doc.documentElement
    const synced = (root.getAttribute('itunes:timing') ?? 'None') !== 'None'
    const lines = []
    for (const stanza of doc.getElementsByTagName('div')) {
      let first = true
      for (const p of stanza.getElementsByTagName('p')) {
        const text = (p.textContent ?? '').replace(/\s+/g, ' ').trim()
        if (!text) continue
        lines.push({
          start: synced ? seconds(p.getAttribute('begin')) : 0,
          end: synced ? seconds(p.getAttribute('end')) : 0,
          text,
          ...(first && lines.length > 0 ? { stanza: true } : {}),
        })
        first = false
      }
    }
    const writers = [...doc.getElementsByTagName('songwriter')]
      .map((each) => (each.textContent ?? '').trim())
      .filter(Boolean)
    return { synced, lines, writers }
  }

  const QUESTIONS = {
    lyrics,
    home,
    new: () => editorial('music'),
    radio: () => editorial('radio'),
    browse,
    curator,
    search,
    collection,
    artist,
    library,
  }
  // An answer, asked of Apple Music again once should it fail (a moment's trouble: its token renewed, a
  // dropped connection); failed only then.
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

  // --- Playing ------------------------------------------------------------------------------------------------

  // MusicKit's queue for what's pressed: an album or playlist (the page's, or the Library's), a station, an
  // artist's top songs, a list of songs, or a song on its own.
  const QUEUES = {
    albums: (key) => ({ album: key }),
    'library-albums': (key) => ({ album: key }),
    playlists: (key) => ({ playlist: key }),
    'library-playlists': (key) => ({ playlist: key }),
    stations: (key) => ({ station: key }),
    songs: (key) => ({ song: key }),
    'library-songs': (key) => ({ song: key }),
  }
  // What plays came from (for Now Playing's "Playing from"), as the app said it.
  const context = { id: null, name: '', kind: null }

  // Playing, as Apple's own player shows it: also while a song loads, waits for its stream or seeks
  // (MusicKit's states 1 loading, 2 playing, 6 seeking, 8 waiting, 9 stalled); not stopped, paused or ended.
  const PLAYING = new Set([1, 2, 6, 8, 9])
  const isPlaying = (music) => PLAYING.has(music.playbackState)
  // Its sound coming: an audio element playing, its time moving (MusicKit's own time is in rounded seconds).
  const audible = () =>
    [...document.querySelectorAll('audio, video')].some(
      (element) => !element.paused && element.currentTime > 0.05 && element.readyState >= 3,
    )
  // How far into the song, to the tenth: MusicKit says whole seconds (rounded), and a full song's audio element
  // keeps no time of its own song. So a clock of this page's: a song starts at 0 as its sound comes, runs on
  // while it plays, and is set to MusicKit's time only when the two part by more than a second (a seek).
  const clock = { item: null, position: 0, at: 0, running: false }
  const timeOf = (music) => {
    const item = music.nowPlayingItem?.id ?? null
    const rough = Number(music.currentPlaybackTime) || 0
    const now = performance.now()
    let position = clock.position + (clock.running ? (now - clock.at) / 1000 : 0)
    if (clock.item !== item) position = rough >= 1 ? rough - 0.5 : 0
    else if (Math.abs(position - rough) > 1.2) position = rough
    Object.assign(clock, { item, position, at: now, running: music.playbackState === 2 && audible() })
    return Math.max(0, position)
  }
  // Loading: playing as asked, the song's sound not come yet (the song loading, its stream waited for). The
  // app holds its time still then, and shows it waiting. Only till a song first sounds: played on after a
  // pause, or sought, its sound is back within a moment, and Apple's own player shows no waiting for that.
  let sounded = null
  const isLoading = (music) => {
    const item = music.nowPlayingItem?.id ?? null
    if (music.playbackState === 2 && audible()) sounded = item
    return isPlaying(music) && item !== sounded
  }
  // While it loads, looked at often: said again the moment its sound comes (MusicKit says nothing then).
  let loadingWatch = null
  const watchLoading = (music) => {
    if (loadingWatch) return
    const since = performance.now()
    loadingWatch = setInterval(() => {
      if (isLoading(music) && performance.now() - since < 30000) return
      clearInterval(loadingWatch)
      loadingWatch = null
      tellPlayback()
    }, 120)
  }

  // A change of song under way (a play, a skip, a jump): MusicKit passes through no song, then the new one
  // stopped, waiting and loading on the way; none of that is said, only where it ends up.
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

  const play = (command) => settle(() => playNow(command))
  // How long a play took: until MusicKit had the queue, and until the song's time first moved (sound); said to
  // the host, which logs it (times only).
  const timePlay = (music, started, queued, action = 'play', before = null) => {
    const done = (sounding) =>
      post({
        timing: {
          action,
          queued,
          sounding,
          full: !music.previewOnly && music.isAuthorized,
          steps: stopSteps(),
        },
      })
    const check = () => {
      // Sounding: playing, its time moving, and (a skip) another song than before.
      const moved = before === null || (music.nowPlayingItem?.id ?? null) !== before
      if (moved && music.playbackState === 2 && audible()) {
        clearInterval(looking)
        clearTimeout(giveUp)
        done(Math.round(performance.now() - started))
      }
    }
    // Looked at often (Apple's own time events come only every quarter second or so).
    const looking = setInterval(check, 30)
    const giveUp = setTimeout(() => {
      clearInterval(looking)
      done(null)
    }, 20000)
    check()
  }

  const playNow = async ({ id, songs, index, shuffle }) => {
    const music = await ready
    const started = performance.now()
    if (id || songs) startSteps()
    if (!id && !songs) {
      await music.play()
      return
    }
    sounded = null
    let queue
    if (songs?.length) {
      queue = { songs: songs.map((song) => split(song)[1]) }
    } else {
      const [type, key] = split(id)
      if (type === 'artists' || type === 'library-artists') {
        const page = await artist({ id })
        queue = { songs: page.top.map((song) => split(song.id)[1]) }
      } else if (QUEUES[type]) {
        queue = QUEUES[type](key)
      } else throw new Error('nothing to play')
    }
    // The song asked for first, then (shuffling) the rest shuffled: queued in order from that song, then
    // shuffled, which keeps the song playing first. (Shuffle on as it's queued, MusicKit counts the start in
    // the shuffled order, and a start of 0 keeps the list's first song first.)
    const shuffled = typeof shuffle === 'boolean' ? shuffle : music.shuffleMode === 1
    music.shuffleMode = 0
    const start = typeof index === 'number' && index > 0 ? index : 0
    await music.setQueue({ ...queue, startWith: start, startPlaying: true })
    timePlay(music, started, Math.round(performance.now() - started))
    if (start > 0 && music.nowPlayingItemIndex !== start) await music.changeToMediaAtIndex(start).catch(() => {})
    if (shuffled) music.shuffleMode = 1
    await music.play().catch(() => {})
    if (id) {
      const [type] = split(id)
      context.id = id
      context.name = names.get(id) ?? ''
      context.kind = KINDS[type] ?? null
    } else {
      context.id = null
    }
  }

  const CONTROLS = {
    play,
    pause: async () => (await ready).pause(),
    next: () =>
      settle(async () => {
        const music = await ready
        const started = performance.now()
        const before = music.nowPlayingItem?.id ?? null
        sounded = null
        startSteps()
        await music.skipToNextItem()
        timePlay(music, started, Math.round(performance.now() - started), 'next', before)
      }),
    // Within a song's first seconds, the one before; later, its start (as Apple's own Back does).
    previous: () =>
      settle(async () => {
        const music = await ready
        if (music.currentPlaybackTime > 3) {
          await music.seekToTime(0)
          return
        }
        const started = performance.now()
        const before = music.nowPlayingItem?.id ?? null
        sounded = null
        startSteps()
        await music.skipToPreviousItem()
        timePlay(music, started, Math.round(performance.now() - started), 'previous', before)
      }),
    seek: async ({ seconds }) => (await ready).seekToTime(Math.max(0, Number(seconds) || 0)),
    volume: async ({ percent }) => {
      ;(await ready).volume = Math.min(1, Math.max(0, Number(percent) / 100))
      tellPlayback()
    },
    shuffle: async ({ on }) => {
      ;(await ready).shuffleMode = on ? 1 : 0
      tellPlayback()
    },
    // MusicKit's repeat: 0 none, 1 the song, 2 all.
    repeat: async ({ mode }) => {
      ;(await ready).repeatMode = mode === 'one' ? 1 : mode === 'all' ? 2 : 0
      tellPlayback()
    },
    // A song of the queue, `offset` songs on.
    jump: ({ offset }) =>
      settle(async () => {
        const music = await ready
        sounded = null
        await music.changeToMediaAtIndex(music.nowPlayingItemIndex + Number(offset))
      }),
    favorite: async ({ id, on }) => {
      const [type, key] = split(id)
      // A favourite is Apple Music's rating of 1 (its "love"): a Library song's own (`library-songs`, as the
      // Library reads them), else the Apple Music song's.
      const path = `/v1/me/ratings/${type}/${key}`
      if (on) await api(path, {}, 'PUT', { type: 'rating', attributes: { value: 1 } })
      else await api(path, {}, 'DELETE')
      favorites.set(id, Boolean(on))
      tellPlayback()
    },
    add: async ({ id }) => {
      const [type, key] = split(id)
      await api('/v1/me/library', { [`ids[${type}]`]: key }, 'POST')
    },
    // Signing out: MusicKit forgets the account (its token, kept in the page's storage), then WestPlay
    // forgets Apple's cookies and loads the page again.
    signOut: async () => {
      const music = await ready
      await music.unauthorize().catch(() => {})
      try {
        localStorage.clear()
        sessionStorage.clear()
      } catch {
        // Nothing kept, or nothing to clear.
      }
      post({ signedOut: true })
    },
  }

  // --- What plays -------------------------------------------------------------------------------------------

  // Songs favourited (or not), as Apple Music said or as changed here. A Library song's rating is its own
  // (`library-songs`; asked as an Apple Music song, it has none). Not answered: asked again next time.
  const favorites = new Map()
  const favoriteOf = async (song) => {
    const music = await ready
    if (!song || !music.isAuthorized) return null
    if (!favorites.has(song.id)) {
      const [type, key] = split(song.id)
      const found = await api(`/v1/me/ratings/${type}/${key}`)
      favorites.set(song.id, found?.data?.[0]?.attributes?.value === 1)
    }
    return favorites.get(song.id)
  }

  // A song of MusicKit's queue, as the app shows songs.
  const queued = (media) => {
    if (!media) return null
    const attributes = media.attributes ?? {}
    // A song of the Library has an id of its own (`i.…`), its Apple Music one a number.
    const kind = media.type === 'library-songs' || String(media.id).startsWith('i.') ? 'library-songs' : 'songs'
    const id = `${kind}:${media.id}`
    return {
      id,
      name: attributes.name ?? media.title ?? '',
      artist: attributes.artistName ?? media.artistName ?? '',
      album: attributes.albumName ?? media.albumName ?? '',
      albumId: albumOf({ attributes, relationships: media.relationships }),
      artistId: artistOf({ attributes, relationships: media.relationships }),
      image: image(attributes.artwork),
      color: color(attributes.artwork),
      duration: Number(attributes.durationInMillis ?? media.playbackDuration ?? 0) / 1000,
      explicit: attributes.contentRating === 'explicit',
      number: attributes.trackNumber ?? null,
    }
  }

  let telling = Promise.resolve()
  const tellPlayback = () => {
    telling = telling
      .then(async () => {
        const music = instance()
        if (!music || changing > 0) return
        const now = music.nowPlayingItem
        if (!now) {
          // Nothing plays only once the queue is empty (between songs, MusicKit has none for a moment).
          if (!music.queue?.items?.length) post({ playback: null })
          return
        }
        const song = queued(now)
        const favorite = await favoriteOf(song).catch(() => null)
        const index = music.nowPlayingItemIndex ?? 0
        const items = music.queue?.items ?? []
        const length = Number(music.currentPlaybackDuration) || song.duration
        const loading = isLoading(music)
        if (loading) watchLoading(music)
        const container = now.container
        const from =
          context.id && (!container?.id || context.id.endsWith(`:${container.id}`))
            ? { id: context.id, name: context.name, kind: context.kind }
            : null
        post({
          playback: {
            track: { ...song, favorite },
            context: from?.kind ? from : null,
            playing: isPlaying(music),
            loading,
            position: timeOf(music),
            duration: length,
            // Apple Music plays a song's preview (its 90 seconds) to an account without a subscription, or
            // signed out.
            preview: song.duration > 0 && length > 0 && length < song.duration - 5,
            shuffle: music.shuffleMode === 1,
            repeat: music.repeatMode === 1 ? 'one' : music.repeatMode === 2 ? 'all' : 'off',
            volume: typeof music.volume === 'number' ? music.volume : null,
            next: items
              .slice(index + 1, index + 11)
              .map(queued)
              .filter(Boolean),
          },
        })
      })
      .catch(() => {})
  }

  // --- The app's commands, and the account --------------------------------------------------------------

  // The truck's own speakers (cab_sound.js, loaded first): Apple's audio element (not the muted videos of
  // its moving covers), told the speakers, the head and the camera by the host, kept on the element there is.
  const cab = window.westplayCabSound?.(() => {
    const audio = [...document.querySelectorAll('audio')]
    return audio.find((each) => !each.paused) ?? audio.at(-1) ?? null
  })
  if (cab) setInterval(() => cab.tick(), 250)
  const CAB = {
    speakers: (data) => cab?.speakers(data.speakers),
    head: (data) => cab?.head(data),
    outside: (data) => cab?.outside(data.on, data.distance),
  }

  window.chrome?.webview?.addEventListener('message', ({ data }) => {
    if (CAB[data?.action]) CAB[data.action](data)
    else if (data?.action === 'ask') answer(data.id, data.question)
    else CONTROLS[data?.action]?.(data)?.catch?.(() => {})
  })

  // The account's picture: its Apple Music profile's (if it has made one), asked once it's signed in.
  const avatar = { url: null, asked: false }
  const askAvatar = async () => {
    avatar.asked = true
    const found = await api('/v1/me/social-profile', { 'art[url]': 'f' }).catch(() => null)
    avatar.url = image(found?.data?.[0]?.attributes?.artwork, 160)
    tellState(true)
  }

  // Signed in or not (and the account's picture), and playing.
  let told = ''
  const tellState = (always = false) => {
    const music = instance()
    if (!music?.storefrontId) return
    const signedIn = Boolean(music.isAuthorized)
    if (signedIn && !avatar.asked) askAvatar()
    if (!signedIn) Object.assign(avatar, { url: null, asked: false })
    const now = JSON.stringify({ signedIn, playing: isPlaying(music), avatar: avatar.url })
    if (now === told && !always) return
    told = now
    post(JSON.parse(now))
  }

  ready.then((music) => {
    // This page is ready (a page loaded anew: the questions it missed are asked again).
    post({ ready: true })
    const EVENTS = [
      'playbackStateDidChange',
      'nowPlayingItemDidChange',
      'queueItemsDidChange',
      'queuePositionDidChange',
      'shuffleModeDidChange',
      'repeatModeDidChange',
      'playbackVolumeDidChange',
    ]
    // MusicKit's own states, as steps while a play is timed.
    const STATES = [
      'none',
      'loading',
      'playing',
      'paused',
      'stopped',
      'ended',
      'seeking',
      '',
      'waiting',
      'stalled',
      'completed',
    ]
    music.addEventListener('playbackStateDidChange', () =>
      step(`mk:${STATES[music.playbackState] ?? music.playbackState}`),
    )
    // The next song prepared a few seconds into each song (not competing with its start), and as the queue
    // changes (shuffled, another list).
    let preparing
    const prepareSoon = () => {
      clearTimeout(preparing)
      preparing = setTimeout(() => {
        if (music.playbackState === 2) prepareNext(music)
      }, PREPARE_AFTER)
    }
    for (const name of ['nowPlayingItemDidChange', 'queueItemsDidChange', 'shuffleModeDidChange']) {
      music.addEventListener(name, prepareSoon)
    }
    music.addEventListener('playbackStateDidChange', () => {
      if (music.playbackState === 2) prepareSoon()
    })
    for (const name of EVENTS) {
      music.addEventListener(name, () => {
        tellPlayback()
        tellState()
      })
    }
    // Seeking moves the time: said again then (the app runs the time on by itself while it plays).
    music.addEventListener('playbackTimeDidChange', () => {
      const time = Number(music.currentPlaybackTime) || 0
      if (Math.abs(time - (lastTime + (Date.now() - lastAt) / 1000)) > 2 && music.playbackState === 2) tellPlayback()
      lastTime = time
      lastAt = Date.now()
    })
    music.addEventListener('authorizationStatusDidChange', () => {
      favorites.clear()
      tellState(true)
      tellPlayback()
    })
    tellState(true)
    tellPlayback()
  })
  let lastTime = 0
  let lastAt = Date.now()
  setInterval(() => tellState(), 1000)
})()
