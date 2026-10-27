// WestPlay, in Spotify's web player (spotify.rs): Spotify's own page, run hidden under the cab. The Spotify app
// the player sees is WestPlay's own; this answers its questions and plays as it asks, from the page itself, as
// the page's own code would:
//
// - Questions (Home, Your Library, Browse all, a search, a playlist's, album's or artist's page) are asked of
//   Spotify's own API (pathfinder) with the page's own session, its queries found in the page's own scripts
//   (each query's name and hash, as the page defines them: they change with every release of the page).
// - Playing something goes through Spotify Connect, the page's own way of telling a device what to play (it
//   plays, itself being the device); its own buttons if Connect isn't there yet. Pausing and playing on, while
//   it plays here, are the page's own button, and the cab's fades its own player's volume: Connect's commands
//   go round Spotify's servers and back, its state comes later still, and a pause and a play on crossing there
//   left the music stopped as the player came back to the game (owner 2026-10-24).
// - What plays comes from Spotify Connect's state, as the page hears it, the songs' names and covers asked
//   for.
//
// Nothing of the account leaves the page: its session (token, client token) and its devices stay here, only
// used as the page uses them. The answers are what the app shows (names, covers, what plays), and the one
// address that holds the account's name (Liked Songs') leaves as `spotify:collection:tracks`. Signed in or
// not is all that's said of the account.
;(() => {
  if (location.hostname !== 'open.spotify.com' || window.top !== window) return
  const post = (message) => window.chrome?.webview?.postMessage(message)
  const pageFetch = window.fetch.bind(window)

  // --- How long playing takes, step by step: for the host's log, the kinds of steps and their times only
  // (Spotify's requests by kind, its player's element's events, its own state), never addresses or contents.
  const steps = { since: null, action: null, list: [], timer: null }
  const step = (what) => {
    if (steps.since !== null && steps.list.length < 60) {
      steps.list.push(`${what} ${Math.round(performance.now() - steps.since)}`)
    }
  }
  const REQUESTS = [
    [/\/player\/command\//, 'command'],
    [/\/connect\/volume/, 'volume'],
    [/widevine-license/, 'licence'],
    [/\/track-playback\//, 'playback'],
    [/\/storage-resolve\//, 'resolve'],
    [/\/(audio|seektables)\//, 'audio'],
  ]
  const kindOf = (url) => REQUESTS.find(([pattern]) => pattern.test(String(url)))?.[1] ?? null

  // --- What the page sends, watched (never changed): its session, its queries, its device -----------------

  const AUTH_HEADERS = ['authorization', 'client-token', 'app-platform', 'spotify-app-version', 'accept-language']
  const auth = { headers: null, waiting: [] }
  const hashes = new Map()
  // Spotify Connect: its address, this page's device (the player's own id, 40 characters), the start of it
  // its observer gives (`hobs_` and the id, cut to 40 characters in all: not the device), and the device
  // playing now.
  const connect = { endpoint: null, device: null, observer: null, active: null }

  const headersOf = (headers) => {
    try {
      return new Headers(headers ?? undefined)
    } catch {
      return new Headers()
    }
  }
  const tookSession = (headers) => {
    auth.headers = Object.fromEntries(
      AUTH_HEADERS.filter((name) => headers.get(name)).map((name) => [name, headers.get(name)]),
    )
    auth.waiting.splice(0).forEach((resolve) => resolve())
  }
  const CONNECT = /^(https:\/\/[^/]+\/connect-state)\/v1\//
  const OBSERVER = /\/connect-state\/v1\/devices\/hobs_([0-9a-f]+)/
  // The device's own id: in the player's own requests (its commands, its playback's registration).
  const DEVICE = /\/(?:player\/command\/from|track-playback\/v1\/devices)\/([0-9a-f]{40})(?:[/?]|$)/
  const seen = (url, headers, body) => {
    try {
      if (url.includes('/pathfinder/')) {
        const sent = headersOf(headers)
        if (sent.get('authorization')) tookSession(sent)
        if (typeof body === 'string') {
          const query = JSON.parse(body)
          const hash = query?.extensions?.persistedQuery?.sha256Hash
          if (query?.operationName && hash) hashes.set(query.operationName, hash)
        }
      }
      const endpoint = url.match(CONNECT)
      if (endpoint) connect.endpoint = endpoint[1]
      const observer = url.match(OBSERVER)
      if (observer) connect.observer = observer[1]
      const device = url.match(DEVICE)
      if (device) connect.device = device[1]
      // The player registering itself: its id in what it sends.
      if (/\/track-playback\/v1\/devices\/?$/.test(url) && typeof body === 'string') {
        const id = JSON.parse(body)?.device?.device_id
        if (/^[0-9a-f]{40}$/.test(id ?? '')) connect.device = id
      }
    } catch {
      // Not ours to understand: the page's request goes on as it was.
    }
  }

  window.fetch = function (input, init) {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : (input?.url ?? '')
    seen(url, init?.headers ?? (input instanceof Request ? input.headers : undefined), init?.body)
    const response = pageFetch(input, init)
    const kind = steps.since === null ? null : kindOf(url)
    if (kind) {
      step(`${kind}>`)
      response.then(
        () => step(kind),
        () => step(`${kind}!`),
      )
    }
    // The page's session renewed, and its device registered (the answer is the account's Connect state).
    if (url.startsWith('https://open.spotify.com/api/token')) {
      response
        .then((r) => r.clone().json())
        .then((token) => {
          if (token?.accessToken && auth.headers) auth.headers.authorization = `Bearer ${token.accessToken}`
        })
        .catch(() => {})
    }
    if (/\/connect-state\/v1\/devices\//.test(url)) {
      response
        .then((r) => r.clone().json())
        .then(heardCluster)
        .catch(() => {})
    }
    return response
  }

  const PAGE_REQUEST = Symbol('westplay')
  const open = XMLHttpRequest.prototype.open
  const setRequestHeader = XMLHttpRequest.prototype.setRequestHeader
  const send = XMLHttpRequest.prototype.send
  XMLHttpRequest.prototype.open = function (method, url) {
    this[PAGE_REQUEST] = { url: String(url), headers: {} }
    return open.apply(this, arguments)
  }
  XMLHttpRequest.prototype.setRequestHeader = function (name, value) {
    if (this[PAGE_REQUEST]) this[PAGE_REQUEST].headers[String(name).toLowerCase()] = value
    return setRequestHeader.apply(this, arguments)
  }
  XMLHttpRequest.prototype.send = function (body) {
    if (this[PAGE_REQUEST]) {
      seen(this[PAGE_REQUEST].url, this[PAGE_REQUEST].headers, body)
      const kind = steps.since === null ? null : kindOf(this[PAGE_REQUEST].url)
      if (kind) {
        step(`${kind}>`)
        this.addEventListener('loadend', () => step(kind), { once: true })
      }
    }
    return send.apply(this, arguments)
  }

  // Spotify Connect's state, as the page hears it on its dealer socket.
  const PageSocket = window.WebSocket
  window.WebSocket = new Proxy(PageSocket, {
    construct(target, args) {
      const socket = Reflect.construct(target, args)
      if (String(args[0]).includes('dealer')) socket.addEventListener('message', (event) => heardDealer(event.data))
      return socket
    },
  })

  const unpacked = async (text, headers) => {
    const bytes = Uint8Array.from(atob(text), (c) => c.charCodeAt(0))
    const gzip = /gzip/i.test(headers?.['Transfer-Encoding'] ?? '') || (bytes[0] === 0x1f && bytes[1] === 0x8b)
    const stream = new Blob([bytes]).stream()
    return JSON.parse(await new Response(gzip ? stream.pipeThrough(new DecompressionStream('gzip')) : stream).text())
  }
  const heardDealer = async (data) => {
    try {
      if (typeof data !== 'string') return
      const message = JSON.parse(data)
      if (!String(message.uri ?? '').startsWith('hm://connect-state/v1/cluster')) return
      let payload = message.payloads?.[0]
      if (typeof payload === 'string') payload = await unpacked(payload, message.headers)
      heardCluster(payload)
    } catch {
      // A message the page reads its own way.
    }
  }

  // --- Its queries, from its scripts -------------------------------------------------------------------------

  // How the page's scripts define each query: `("home","query","<sha256>",…)`.
  const DEFINITION = /"([A-Za-z0-9]+)","(?:query|mutation)","([0-9a-f]{64})"/g
  const SCRIPTS = /^https:\/\/open\.spotifycdn\.com\/cdn\/build\/web-player\/[^?#]+\.js$/
  const scanned = new Set()
  const scan = async () => {
    const urls = [
      ...performance.getEntriesByType('resource').map((entry) => entry.name),
      ...[...document.scripts].map((script) => script.src),
    ].filter((url) => SCRIPTS.test(url) && !scanned.has(url))
    for (const url of new Set(urls)) {
      scanned.add(url)
      try {
        const text = await (await pageFetch(url)).text()
        for (const [, name, hash] of text.matchAll(DEFINITION)) if (!hashes.has(name)) hashes.set(name, hash)
      } catch {
        scanned.delete(url)
      }
    }
  }
  const hashOf = async (name) => {
    if (!hashes.has(name)) await scan()
    return hashes.get(name)
  }

  // The page's session: there as soon as the page has asked Spotify anything (it does as it loads).
  const SESSION_WAIT = 20000
  const session = () =>
    auth.headers
      ? Promise.resolve()
      : new Promise((resolve, reject) => {
          auth.waiting.push(resolve)
          setTimeout(() => reject(new Error('no session')), SESSION_WAIT)
        })

  const query = async (name, variables) => {
    await session()
    const hash = await hashOf(name)
    if (!hash) throw new Error(`no query ${name}`)
    const ask = () =>
      pageFetch('https://api-partner.spotify.com/pathfinder/v2/query', {
        method: 'POST',
        headers: {
          ...auth.headers,
          'content-type': 'application/json;charset=UTF-8',
          accept: 'application/json',
        },
        body: JSON.stringify({
          variables,
          operationName: name,
          extensions: { persistedQuery: { version: 1, sha256Hash: hash } },
        }),
      })
    let response = await ask()
    // The session ran out: the page renews it; asked again with the new one.
    if (response.status === 401) {
      auth.headers = null
      await session()
      response = await ask()
    }
    if (!response.ok) throw new Error(`${name}: ${response.status}`)
    const json = await response.json()
    if (!json?.data) throw new Error(`${name}: no data`)
    return json.data
  }

  // --- Spotify's answers, cut down to what the app shows ------------------------------------------------------

  const FALLBACK = '#535353'
  const LIKED = 'spotify:collection:tracks'
  // Liked Songs' own address (it holds the account's name): kept here, never told.
  let likedUri = null
  let likedName = null

  const outward = (uri) => {
    if (!uri) return uri
    if (/^spotify:user:[^:]+:collection$/.test(uri) || uri === likedUri) {
      likedUri = likedUri ?? uri
      return LIKED
    }
    const old = uri.match(/^spotify:user:[^:]+:playlist:([A-Za-z0-9]+)$/)
    return old ? `spotify:playlist:${old[1]}` : uri
  }
  const inward = (uri) => (uri === LIKED ? (likedUri ?? uri) : uri)

  const unwrap = (wrapper) => wrapper?.data ?? wrapper ?? null
  const image = (sources) => {
    const list = (sources ?? []).filter((source) => source?.url)
    if (list.length === 0) return null
    const sized = list.filter((source) => source.width).sort((a, b) => a.width - b.width)
    return (sized.find((source) => source.width >= 300) ?? sized.at(-1) ?? list[0]).url
  }
  const colour = (colours) => {
    const found = colours?.colorDark ?? colours?.colorRaw ?? colours?.colorLight
    return found?.hex && !found.isFallback ? found.hex : FALLBACK
  }
  const names = (artists) => (artists?.items ?? []).map((artist) => artist?.profile?.name).filter(Boolean)
  // Spotify's descriptions are HTML (links to other playlists): their words only.
  const words = (html) => {
    if (!html) return ''
    const page = new DOMParser().parseFromString(String(html), 'text/html')
    return (page.body.textContent ?? '').replace(/\s+/g, ' ').trim()
  }

  const track = (data, album) => {
    if (data?.__typename && data.__typename !== 'Track') return null
    if (!data?.uri?.startsWith('spotify:track:')) return null
    const of = data.albumOfTrack ?? album ?? {}
    const cover = of.coverArt ?? album?.coverArt
    return {
      uri: data.uri,
      name: data.name ?? '',
      artists: (data.artists?.items ?? data.firstArtist?.items ?? [])
        .map((artist) => ({
          uri: artist.uri,
          name: artist.profile?.name ?? '',
        }))
        .filter((artist) => artist.uri && artist.name),
      album: {
        uri: of.uri ?? album?.uri ?? '',
        name: of.name ?? album?.name ?? '',
      },
      image: image(cover?.sources),
      color: colour(cover?.extractedColors),
      duration: Number((data.duration ?? data.trackDuration)?.totalMilliseconds ?? 0) / 1000,
      explicit: data.contentRating?.label === 'EXPLICIT',
      ...(typeof data.saved === 'boolean' ? { saved: data.saved } : {}),
    }
  }

  const item = (data, extra = {}) => {
    if (!data) return null
    const base = { by: '', description: '', ...extra }
    switch (data.__typename) {
      case 'Playlist': {
        const picture = data.images?.items?.[0]
        return {
          ...base,
          uri: outward(data.uri),
          kind: 'playlist',
          name: data.name ?? '',
          by: unwrap(data.ownerV2)?.name ?? '',
          description: words(data.description),
          image: image(picture?.sources),
          color: colour(picture?.extractedColors),
        }
      }
      case 'Album': {
        const by = names(data.artists).join(', ')
        return {
          ...base,
          uri: data.uri,
          kind: 'album',
          name: data.name ?? '',
          by,
          description: by,
          image: image(data.coverArt?.sources),
          color: colour(data.coverArt?.extractedColors),
        }
      }
      case 'Artist':
        return {
          ...base,
          uri: data.uri,
          kind: 'artist',
          name: data.profile?.name ?? '',
          image: image(data.visuals?.avatarImage?.sources),
          color: colour(data.visuals?.avatarImage?.extractedColors),
        }
      case 'Podcast':
        return {
          ...base,
          uri: data.uri,
          kind: 'show',
          name: data.name ?? '',
          by: data.publisher?.name ?? '',
          image: image(data.coverArt?.sources),
          color: colour(data.coverArt?.extractedColors),
        }
      case 'PseudoPlaylist': {
        // Liked Songs (`spotify:collection:tracks`; the older `spotify:user:…:collection`); not Your Episodes.
        if (data.uri !== LIKED && !/^spotify:user:[^:]+:collection$/.test(data.uri ?? '')) return null
        likedUri = data.uri
        likedName = data.name ?? likedName
        return {
          ...base,
          uri: LIKED,
          kind: 'liked',
          name: data.name ?? '',
          songs: Number(data.count ?? 0),
          image: null,
          color: colour(data.image?.extractedColors),
        }
      }
      case 'Track': {
        const song = track(data)
        if (!song) return null
        return {
          ...base,
          uri: song.uri,
          kind: 'track',
          name: song.name,
          by: song.artists.map((artist) => artist.name).join(', '),
          description: song.artists.map((artist) => artist.name).join(', '),
          image: song.image,
          color: song.color,
          context: song.album.uri,
        }
      }
      default:
        return null
    }
  }
  const items = (list, pick = (entry) => entry) =>
    (list ?? []).map((entry) => item(unwrap(pick(entry)))).filter((found) => found?.uri && found.name)
  const label = (title) =>
    title?.transformedLabel ?? title?.translatedBaseText ?? title?.text ?? (typeof title === 'string' ? title : '')

  const home = async ({ facet }) => {
    const data = await query('home', {
      homeEndUserIntegration: 'INTEGRATION_WEB_PLAYER',
      timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      sp_t: '',
      facet: facet ?? '',
      sectionItemsLimit: 10,
      includeHomeChapterVideoCards: false,
      includeEpisodeContentRatingsV2: false,
    })
    const page = data.home ?? {}
    const sections = page.sectionContainer?.sections?.items ?? []
    let shortcuts = []
    const shelves = []
    for (const section of sections) {
      const found = items(section.sectionItems?.items, (entry) => entry.content)
      if (found.length === 0) continue
      if (/Short/i.test(section.data?.__typename ?? '')) shortcuts = shortcuts.length ? shortcuts : found
      else {
        // Spotify's Recently played shelf, as its own data names it (the app keeps it to show at once).
        const recent = /RecentlyPlayed/i.test(section.data?.__typename ?? '')
        shelves.push({ title: label(section.data?.title), items: found, ...(recent ? { recent } : {}) })
      }
    }
    const chips = (page.homeChips ?? [])
      .map((chip) => ({
        id: String(chip?.id ?? ''),
        label: label(chip?.label),
      }))
      .filter((chip) => chip.id && chip.label)
    return {
      chips,
      shortcuts: shortcuts.slice(0, 8),
      sections: shelves.filter((shelf) => shelf.title),
    }
  }

  // A song's words: Spotify's own (its lyrics service, as its web player asks it, with the page's session), line
  // by line with when each is sung, where they're from, and Spotify's colours for them; none (an empty answer)
  // for a song without words.
  const argb = (value) => {
    const number = Number(value)
    return Number.isFinite(number) ? `#${((number >>> 0) & 0xffffff).toString(16).padStart(6, '0')}` : null
  }
  const lyrics = async ({ uri }) => {
    const id = /^spotify:track:([0-9A-Za-z]{22})$/.exec(uri ?? '')?.[1]
    if (!id) return { synced: false, lines: [] }
    await session()
    const response = await pageFetch(
      `https://spclient.wg.spotify.com/color-lyrics/v2/track/${id}?format=json&vocalRemoval=false&market=from_token`,
      { headers: { ...auth.headers, 'app-platform': 'WebPlayer', accept: 'application/json' } },
    )
    // No words for the song.
    if (response.status === 404) return { synced: false, lines: [] }
    if (!response.ok) throw new Error(`lyrics: ${response.status}`)
    const json = await response.json()
    const found = json?.lyrics ?? {}
    const synced = found.syncType === 'LINE_SYNCED'
    const colors = json?.colors
    const [background, text, highlight] = [colors?.background, colors?.text, colors?.highlightText].map(argb)
    return {
      synced,
      lines: (found.lines ?? []).map((line) => ({
        start: synced ? Number(line.startTimeMs) / 1000 || 0 : 0,
        end: synced ? Number(line.endTimeMs) / 1000 || 0 : 0,
        text: String(line.words ?? ''),
      })),
      provider: String(found.providerDisplayName ?? found.provider ?? ''),
      ...(background && text && highlight ? { colors: { background, text, highlight } } : {}),
      rtl: Boolean(found.isRtlLanguage),
    }
  }

  const library = async () => {
    const data = await query('libraryV3', {
      filters: [],
      order: null,
      textFilter: null,
      features: ['LIKED_SONGS', 'YOUR_EPISODES_V2', 'PRERELEASES', 'PRERELEASES_V2', 'EVENTS'],
      limit: 50,
      offset: 0,
      flatten: false,
      expandedFolders: [],
      folderUri: null,
      includeFoldersWhenFlattening: true,
    })
    return (data.me?.libraryV3?.items ?? [])
      .map((entry) => item(unwrap(entry.item), entry.pinned ? { pinned: true } : {}))
      .filter((found) => found?.uri && found.name)
  }

  const browse = async () => {
    const data = await query('browseAll', {
      browseEndUserIntegration: 'INTEGRATION_WEB_PLAYER',
      pagePagination: { offset: 0, limit: 10 },
      sectionPagination: { offset: 0, limit: 99 },
    })
    const tiles = (data.browseStart?.sections?.items ?? []).flatMap((section) => section.sectionItems?.items ?? [])
    return tiles
      .map((tile) => {
        // A tile's card: `content.data.data.cardRepresentation` (a section container, wrapped twice).
        const content = unwrap(tile.content)
        const card = content?.data?.cardRepresentation ?? content?.cardRepresentation
        const name = label(card?.title)
        if (!name) return null
        const color = card.backgroundColor?.hex ?? FALLBACK
        const art = image(card.artwork?.sources)
        return {
          name,
          color,
          cover: art
            ? {
                uri: tile.uri ?? name,
                kind: 'playlist',
                name,
                by: '',
                description: '',
                image: art,
                color,
              }
            : null,
        }
      })
      .filter(Boolean)
  }

  const search = async ({ query: term }) => {
    const data = await query('searchDesktop', {
      searchTerm: term,
      offset: 0,
      limit: 10,
      numberOfTopResults: 5,
      includeAudiobooks: false,
      includeArtistHasConcertsField: false,
      includePreReleases: false,
      includeLocalConcertsField: false,
      includeAuthors: false,
    })
    const found = data.searchV2 ?? {}
    const tracks = (found.tracksV2?.items ?? []).map((entry) => track(unwrap(entry.item))).filter(Boolean)
    let top = null
    let topTrack = null
    for (const entry of found.topResultsV2?.itemsV2 ?? []) {
      const entity = unwrap(entry.item)
      if (entity?.__typename === 'Track') topTrack = track(entity)
      else top = item(entity)
      if (top || topTrack) break
    }
    if (top) topTrack = null
    return {
      top,
      topTrack,
      tracks,
      artists: items(found.artists?.items),
      playlists: [...items(found.playlists?.items), ...items(found.podcasts?.items)],
      albums: items(found.albumsV2?.items),
    }
  }

  // Where a page's songs go on after these (`count` of them read from `offset`, of `total`): the offset to ask
  // more from (the question's `more`), while there are more. Spotify's own player asks the next as its list is
  // scrolled near its end; Liked Songs and long playlists have thousands (a player's stopped at the first 50).
  const after = (offset, count, total, limit) => {
    const next = offset + count
    const more = Number.isFinite(total) ? next < total : count === limit
    return count > 0 && more ? String(next) : undefined
  }

  const collection = async ({ uri, more }) => {
    const offset = Number(more ?? 0) || 0
    if (uri === LIKED) {
      const limit = 50
      const data = await query('fetchLibraryTracks', { offset, limit })
      const page = data.me?.library?.tracks
      const tracks = (page?.items ?? [])
        .map((entry) => track({ uri: entry.track?._uri, ...unwrap(entry.track) }))
        .filter(Boolean)
        .map((song) => ({ ...song, saved: true }))
      return {
        item: {
          uri: LIKED,
          kind: 'liked',
          name: likedName ?? '',
          by: '',
          description: '',
          songs: Number(page?.totalCount ?? tracks.length),
          image: null,
          color: '#5038a0',
        },
        description: '',
        owner: '',
        year: null,
        tracks,
        more: after(offset, page?.items?.length ?? 0, Number(page?.totalCount), limit),
      }
    }
    if (uri.startsWith('spotify:album:')) {
      const limit = 50
      const album = (await query('getAlbum', { uri, locale: '', offset, limit })).albumUnion
      if (!album || album.__typename !== 'Album') throw new Error('no album')
      const by = names(album.artists).join(', ')
      const year = album.date?.isoString ? new Date(album.date.isoString).getUTCFullYear() : (album.date?.year ?? null)
      return {
        item: {
          ...item(album),
          ...(typeof album.saved === 'boolean' ? { saved: album.saved } : {}),
        },
        description: '',
        owner: by,
        year,
        tracks: (album.tracksV2?.items ?? []).map((entry) => track(entry.track, album)).filter(Boolean),
        more: after(offset, album.tracksV2?.items?.length ?? 0, Number(album.tracksV2?.totalCount), limit),
      }
    }
    if (uri.startsWith('spotify:playlist:')) {
      const limit = 100
      const playlist = (
        await query('fetchPlaylist', {
          uri,
          offset,
          limit,
          enableWatchFeedEntrypoint: false,
        })
      ).playlistV2
      if (!playlist || playlist.__typename !== 'Playlist') throw new Error('no playlist')
      const shown = item(playlist)
      return {
        item: shown,
        description: shown.description,
        owner: shown.by,
        year: null,
        tracks: (playlist.content?.items ?? []).map((entry) => track(unwrap(entry.itemV2))).filter(Boolean),
        more: after(offset, playlist.content?.items?.length ?? 0, Number(playlist.content?.totalCount), limit),
      }
    }
    throw new Error('not a page the app shows')
  }

  const artist = async ({ uri }) => {
    const found = (
      await query('queryArtistOverview', {
        uri,
        locale: '',
        preReleaseV2: false,
      })
    ).artistUnion
    if (!found || found.__typename !== 'Artist') throw new Error('no artist')
    return {
      item: {
        ...item({ ...found, __typename: 'Artist' }),
        ...(typeof found.saved === 'boolean' ? { saved: found.saved } : {}),
      },
      listeners: Number(found.stats?.monthlyListeners ?? 0),
      verified: Boolean(found.profile?.verified ?? found.onPlatformReputationTrait?.verification?.isVerified),
      popular: (found.discography?.topTracks?.items ?? [])
        .map((entry) => track(entry.track))
        .filter(Boolean)
        .slice(0, 5),
      releases: (found.discography?.popularReleasesAlbums?.items ?? [])
        .map((album) =>
          item({
            ...album,
            __typename: 'Album',
            artists: album.artists ?? { items: [{ profile: found.profile }] },
          }),
        )
        .filter(Boolean),
    }
  }

  const QUESTIONS = { home, library, browse, search, collection, artist, lyrics }
  const answer = async (id, question) => {
    try {
      const asked = QUESTIONS[question?.ask]
      if (!asked) throw new Error('not a question')
      post({ answer: id, [question.ask]: await asked(question) })
    } catch {
      post({ answer: id, failed: true })
    }
  }

  // --- Playing ----------------------------------------------------------------------------------------------

  const commandId = () =>
    [...crypto.getRandomValues(new Uint8Array(16))].map((b) => b.toString(16).padStart(2, '0')).join('')

  // A Connect request from this page's device, to it (playing here) or to the one playing (the controls).
  const connectRequest = async (method, path, body, here) => {
    if (!connect.endpoint || !connect.device) throw new Error('no device')
    await session()
    const to = here ? connect.device : (connect.active ?? connect.device)
    const response = await pageFetch(`${connect.endpoint}/v1/${path}/from/${connect.device}/to/${to}`, {
      method,
      headers: {
        authorization: auth.headers.authorization,
        'client-token': auth.headers['client-token'],
        'content-type': 'application/json',
      },
      body: JSON.stringify(body),
    })
    if (!response.ok) throw new Error(`${path}: ${response.status}`)
  }
  const command = (endpoint, fields = {}, here = false) =>
    connectRequest(
      'POST',
      'player/command',
      { command: { ...fields, endpoint, logging_params: { command_id: commandId() } } },
      here,
    )

  // The page's own buttons (whether it was there to press).
  const press = (id) => {
    const button = document.querySelector(`[data-testid="${id}"]`)
    button?.click()
    return Boolean(button)
  }

  // Playing on this page's device (WestPlay's), not another of the account's (a phone).
  const here = () => !connect.active || !connect.device || connect.active === connect.device
  // Whether Spotify's player plays in this page now: the element it plays from, playing (or loading to play on).
  // Its own state, as it changes, where Connect's comes from Spotify's servers a moment later (and one sent before
  // a pause can still say playing after it). Not the OS's media controls: Spotify leaves them at "none" here.
  const playingLocally = () => {
    const element = playingNow()
    return Boolean(element && !element.paused && !element.ended)
  }
  // Paused or played on here: the page's own button, as its own state says it's needed (its own player at once,
  // no round trip); Connect if the button isn't there.
  // Paused at once by the button; played on by Connect, to this page (its Play button, pressed here, only tells
  // Spotify's servers: nothing played in 20 s). A play on still on its way as the player leaves the game again
  // finds the element paused: the pause asked last is kept as it lands (`pausedAt`).
  let pausedAt = null
  // An element silenced while its pause is kept (and its volume to give back): unheard until it's paused, or until
  // the cab asks anything else of it.
  let held = null
  const release = () => {
    if (held) held.element.volume = held.volume
    held = null
  }
  const toggleHere = async (playing, endpoint) => {
    if (playing) release()
    pausedAt = playing ? null : performance.now()
    if (playingLocally() === playing) return
    if (!playing && press('control-button-playpause')) {
      step('button')
      return
    }
    step('command')
    await command(endpoint, {}, true)
  }
  // The element starting a moment after a pause asked here, no play on asked since: paused again at once, by
  // Connect (as it starts, the page's own button still says Play: pressed, it played on).
  const keepPaused = () => {
    if (pausedAt === null || performance.now() - pausedAt > 5000 || !here()) return
    step('kept paused')
    const element = playingNow()
    if (element && !held) {
      held = { element, volume: element.volume }
      element.volume = 0
      element.addEventListener('pause', release, { once: true })
    }
    command('pause', {}, true).catch(() => {})
  }

  // The cab's fade (leaving the game, coming back: musicWithGame.ts), here in the page: the playing element's
  // volume, from Spotify's own (`level` 1) to silence and back, at once. Spotify's own volume, each change a
  // round trip to its servers, came late and out of step with thirty steps a second; it stays as chosen.
  let faded = null
  const fadeTo = ({ level }) => {
    release()
    const element = playingNow()
    if (!element) return
    const share = Math.min(1, Math.max(0, Number(level) || 0))
    // Spotify plays on in another element after a pause (it readies the next song in one): the one faded
    // before back as it was, and this one faded from its own.
    if (faded?.element !== element) {
      if (faded) faded.element.volume = faded.base
      faded = share < 1 ? { element, base: element.volume } : null
    }
    if (!faded) return
    element.volume = faded.base * share
    if (share >= 1) faded = null
  }

  // A play timed (`action`: play, resume, next, previous): from the cab's command until the song's time moves in
  // the element playing it (not the one playing as it was asked, unless it plays on), said to the host with its
  // steps; nothing in 20 s, said so. A new one takes over from one still waiting.
  const timePlay = (action) => {
    clearInterval(steps.timer)
    steps.since = performance.now()
    steps.action = action
    steps.list = []
    const before = playingNow()
    const beforeAt = before && !before.paused ? before.currentTime : null
    let last = null
    steps.timer = setInterval(() => {
      const took = Math.round(performance.now() - steps.since)
      const element = playingNow()
      const moving = Boolean(element && !element.paused && last?.[0] === element && element.currentTime > last[1])
      last = element ? [element, element.currentTime] : null
      const fresh =
        action === 'resume' || element !== before || beforeAt === null || Math.abs(element.currentTime - beforeAt) > 1
      if ((moving && fresh) || took > 20000) {
        clearInterval(steps.timer)
        post({ timing: { action, sounding: moving && fresh ? took : null, steps: steps.list.join(', ') } })
        steps.since = null
      }
    }, 100)
  }

  // Playing on the page itself (Connect not there, or refusing): the page goes to what's to play (in its own
  // way, without loading again) and its own Play is pressed, or the song's own in its list.
  const PAGES = { album: 'album', playlist: 'playlist', artist: 'artist', show: 'show', track: 'track' }
  const pageOf = (uri) => {
    if (uri === LIKED || /:collection$/.test(uri)) return '/collection/tracks'
    const [, kind, id] = uri.split(':')
    return PAGES[kind] && id ? `/${PAGES[kind]}/${id}` : null
  }
  const appears = (find, wait = 8000) =>
    new Promise((resolve) => {
      const started = Date.now()
      const look = () => {
        const found = find()
        if (found || Date.now() - started > wait) resolve(found ?? null)
        else setTimeout(look, 150)
      }
      look()
    })
  const playOnPage = async (context, song) => {
    const page = pageOf(song && !context ? song : context)
    if (!page) throw new Error('no page')
    if (location.pathname !== page) {
      history.pushState(null, '', page)
      dispatchEvent(new PopStateEvent('popstate', { state: null }))
    }
    const id = song?.split(':')[2]
    const row = id
      ? await appears(() => document.querySelector(`[data-testid="tracklist-row"] a[href*="/track/${id}"]`), 5000)
      : null
    const button = row
      ? row.closest('[data-testid="tracklist-row"]')?.querySelector('button')
      : await appears(() => document.querySelector('[data-testid="action-bar-row"] [data-testid="play-button"]'))
    if (!button) throw new Error('no play button')
    button.click()
  }

  // The page's own volume slider, should Connect not be there.
  const slideVolume = (share) => {
    const input = document.querySelector('[data-testid="volume-bar"] input[type="range"]')
    if (!input) return
    const max = Number(input.max || 1)
    const min = Number(input.min || 0)
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(
      input,
      String(min + share * (max - min)),
    )
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new Event('change', { bubbles: true }))
  }

  // The volume asked for and not sent yet (`null`: none), and whether a request is out. The cab fades the music
  // out and in by its volume, a step every 30 ms, each one a request to Spotify's servers: sent all at once they
  // could land out of order, leaving the music at one of the fade's steps, or silent, as the player came back to
  // the game (owner 2026-10-23: "Alt tab, back, playback stops altogether"). One at a time, the newest asked for
  // going out once the last is answered, Spotify ends on the volume asked for last.
  let volumeWanted = null
  let volumeSending = false
  const sendVolume = async () => {
    volumeSending = true
    try {
      while (volumeWanted !== null) {
        const share = volumeWanted
        volumeWanted = null
        await connectRequest('PUT', 'connect/volume', { volume: Math.round(share * 65535) }, false).catch(() =>
          slideVolume(share),
        )
      }
    } finally {
      volumeSending = false
    }
  }

  const play = async ({ context, track: song }) => {
    if (!context) {
      if (here()) {
        if (playingLocally()) return
        timePlay('resume')
        await toggleHere(true, 'resume')
        return
      }
      if (player.state?.playing) return
      await command('resume').catch(() => press('control-button-playpause'))
      return
    }
    pausedAt = null
    release()
    timePlay('play')
    step('command')
    // Liked Songs plays by its own address, which the page learns from Your Library.
    if (context === LIKED && !likedUri) await library().catch(() => {})
    const uri = inward(context)
    await command(
      'play',
      {
        context: { uri, url: `context://${uri}`, metadata: {} },
        play_origin: { feature_identifier: uri.split(':')[1] ?? 'playlist' },
        options: {
          license: 'on-demand',
          skip_to: song ? { track_uri: song } : {},
          player_options_override: {},
        },
      },
      true,
    ).catch(() => playOnPage(uri, song))
  }

  const CONTROLS = {
    play,
    pause: () => {
      if (here()) return toggleHere(false, 'pause')
      return player.state && !player.state.playing
        ? null
        : command('pause').catch(() => press('control-button-playpause'))
    },
    next: () => {
      pausedAt = null
      release()
      timePlay('next')
      return command('skip_next').catch(() => press('control-button-skip-forward'))
    },
    previous: () => {
      pausedAt = null
      release()
      timePlay('previous')
      return command('skip_prev').catch(() => press('control-button-skip-back'))
    },
    fade: fadeTo,
    seek: ({ seconds }) => command('seek_to', { value: Math.max(0, Math.round(seconds * 1000)) }),
    // Spotify Connect's volume runs to 65535. One request at a time, only the newest asked for sent once the last is
    // answered (`sendVolume`).
    volume: ({ percent }) => {
      volumeWanted = Math.min(1, Math.max(0, percent / 100))
      if (!volumeSending) return sendVolume()
    },
    shuffle: ({ on }) =>
      command('set_shuffling_context', { value: Boolean(on) }).catch(() => press('control-button-shuffle')),
    repeat: ({ mode }) =>
      command('set_options', {
        repeating_context: mode !== 'off',
        repeating_track: mode === 'one',
      }),
    save: async ({ uri, on }) => {
      await query(on ? 'addToLibrary' : 'removeFromLibrary', {
        libraryItemUris: [inward(uri)],
      })
      const known = songs.get(uri)
      if (known) songs.set(uri, { ...known, saved: Boolean(on) })
    },
  }

  // --- What plays -------------------------------------------------------------------------------------------

  const player = { state: null }
  // Songs' names and covers, asked for once each (this session).
  const songs = new Map()
  const contexts = new Map()

  const heardCluster = (payload) => {
    const cluster = payload?.cluster ?? payload
    const state = cluster?.player_state
    if (!state) return
    if (cluster.active_device_id) connect.active = cluster.active_device_id
    // This page's device among the account's (its id, of which the observer gave the start).
    const devices = Object.keys(cluster.devices ?? {})
    if (!connect.device && connect.observer) {
      connect.device = devices.find((id) => id.length === 40 && id.startsWith(connect.observer)) ?? null
    }
    // Connect leaves a volume of 0 out (it was read as none: no volume bar, and the volume kept was never given
    // back, the song playing on unheard).
    step(`cluster:${state.is_playing && !state.is_paused ? 'playing' : 'paused'}`)
    const device = cluster.devices?.[cluster.active_device_id]
    const volume = device && !device.capabilities?.disable_volume ? (device.volume ?? 0) : null
    const serverNow = Number(cluster.server_timestamp_ms ?? 0)
    player.state = {
      track: state.track?.uri ?? null,
      context: state.context_uri ?? null,
      contextName: state.context_metadata?.context_description ?? null,
      playing: Boolean(state.is_playing) && !state.is_paused,
      position: Number(state.position_as_of_timestamp ?? 0),
      // Where the position was, in this PC's time (the server's own clock may differ from it).
      at: Number(state.timestamp ?? 0) + (serverNow ? Date.now() - serverNow : 0),
      duration: Number(state.duration ?? 0),
      shuffle: Boolean(state.options?.shuffling_context),
      repeat: state.options?.repeating_track ? 'one' : state.options?.repeating_context ? 'all' : 'off',
      next: (state.next_tracks ?? [])
        .map((next) => next.uri)
        .filter((uri) => uri?.startsWith('spotify:track:'))
        .slice(0, 10),
      // The playing device's volume, 0 to 1 (`null`: it has none Spotify can set).
      volume: typeof volume === 'number' ? volume / 65535 : null,
    }
    tellPlayback()
  }

  const describe = async (uris) => {
    const missing = uris.filter((uri) => !songs.has(uri))
    if (missing.length === 0) return
    // The song playing in full (its colour, whether it's liked); the ones to come in one go.
    const [current, ...rest] = missing
    if (current === player.state?.track) {
      const found = (await query('getTrack', { uri: current }).catch(() => null))?.trackUnion
      const song = found
        ? track({
            ...found,
            artists: found.artists ?? {
              items: [...(found.firstArtist?.items ?? []), ...(found.otherArtists?.items ?? [])],
            },
          })
        : null
      if (song) songs.set(current, song)
    } else rest.unshift(current)
    if (rest.length) {
      const found = (await query('decorateContextTracks', { uris: rest }).catch(() => null))?.tracks ?? []
      for (const data of found) {
        const song = track(data)
        if (song) songs.set(song.uri, song)
      }
    }
  }

  const contextOf = async (uri, name) => {
    if (!uri) return null
    const out = outward(uri)
    const kind = out === LIKED ? 'liked' : (uri.split(':')[1] ?? 'playlist')
    if (!contexts.has(out)) {
      let found = name ?? null
      if (!found && out === LIKED) found = likedName
      if (!found && kind === 'playlist')
        found = (
          await query('fetchPlaylistMetadata', {
            uri,
            offset: 0,
            limit: 1,
            enableWatchFeedEntrypoint: false,
          }).catch(() => null)
        )?.playlistV2?.name
      if (!found && kind === 'album')
        found = (
          await query('getAlbum', {
            uri,
            locale: '',
            offset: 0,
            limit: 1,
          }).catch(() => null)
        )?.albumUnion?.name
      if (!found && kind === 'artist')
        found = (
          await query('queryArtistOverview', {
            uri,
            locale: '',
            preReleaseV2: false,
          }).catch(() => null)
        )?.artistUnion?.profile?.name
      contexts.set(out, found ?? '')
    }
    return {
      uri: out,
      name: contexts.get(out),
      kind: ['playlist', 'album', 'artist', 'show', 'liked'].includes(kind) ? kind : 'playlist',
    }
  }

  let telling = Promise.resolve()
  const tellPlayback = () => {
    telling = telling.then(async () => {
      const state = player.state
      if (!state) return
      if (!state.track) {
        post({ playback: null })
        return
      }
      await describe([state.track, ...state.next]).catch(() => {})
      const position = (state.position + (state.playing ? Math.max(0, Date.now() - state.at) : 0)) / 1000
      post({
        playback: {
          track: songs.get(state.track) ?? null,
          context: await contextOf(state.context, state.contextName).catch(() => null),
          playing: state.playing,
          position,
          duration: state.duration / 1000,
          shuffle: state.shuffle,
          repeat: state.repeat,
          volume: state.volume,
          // Playing on this page's device (WestPlay's), not another of the account's (a phone): only that
          // pauses with the game.
          here: here(),
          next: state.next.map((uri) => songs.get(uri)).filter(Boolean),
        },
      })
    })
  }

  // --- The app's commands, and the account --------------------------------------------------------------

  // The truck's own speakers (cab_sound.js, loaded first): the element Spotify plays
  // from, made by its own script and maybe never put in the page. Kept as Spotify gives it the keys to its
  // protected music (it must, to play it), as it's told to play, or as one in the page starts; told the
  // speakers, the head and the camera by the host.
  let sounding = null
  const known = new Set()
  const MEDIA_EVENTS = ['waitingforkey', 'loadedmetadata', 'canplay', 'playing', 'pause', 'waiting', 'stalled']
  const keep = (element) => {
    if (!(element instanceof HTMLMediaElement)) return
    sounding = element
    if (known.has(element)) return
    known.add(element)
    for (const name of MEDIA_EVENTS) element.addEventListener(name, () => step(name))
    // Playing or not said at once as it changes: the cab's fade in waits for it.
    for (const name of ['play', 'pause', 'ended']) element.addEventListener(name, () => tellState())
    element.addEventListener('playing', () => keepPaused())
  }
  // The one playing now: Spotify readies the next song in another element (given its keys ahead), and plays
  // on in either after a pause. Never a muted one: a song's Canvas (its looping video, in the page) plays
  // muted beside the song, and taken, the speakers played its silence. None playing, the last touched.
  const playingNow = () =>
    [...known].find((element) => !element.paused && !element.ended && !element.muted) ?? sounding
  const mediaPlay = HTMLMediaElement.prototype.play
  HTMLMediaElement.prototype.play = function (...args) {
    keep(this)
    step('mediaplay')
    return mediaPlay.apply(this, args)
  }
  const setMediaKeys = HTMLMediaElement.prototype.setMediaKeys
  HTMLMediaElement.prototype.setMediaKeys = function (...args) {
    keep(this)
    return setMediaKeys.apply(this, args)
  }
  addEventListener('playing', (event) => keep(event.target), true)
  const cab = window.westplayCabSound?.(() => playingNow() ?? document.querySelector('audio, video'), post)
  // What the speakers made of it, said once each way for the host's log: the element found and played
  // through them, none found while Spotify plays, or Spotify playing while the speakers hear nothing for 4 s
  // on end (with how many elements it has played from, and whether the one taken is playing).
  let saidCab = ''
  let quietSince = null
  const tellCab = () => {
    const playing = playingLocally()
    const found = playingNow() ?? document.querySelector('audio, video')
    const levels = cab?.level()
    // Anything at all: Spotify quiet (its volume at 10%) is some 0.0001, its sound failed nothing.
    const heard = Array.isArray(levels) && levels.slice(0, 2).some((ear) => ear > 0.00001)
    quietSince = playing && levels && !heard ? (quietSince ?? Date.now()) : null
    const now =
      quietSince && Date.now() - quietSince > 4000
        ? `silent while playing (${known.size} elements, taken one ${found?.paused ? 'paused' : 'playing'})`
        : levels
          ? `through the speakers (${found?.tagName.toLowerCase() ?? 'media'})`
          : playing && !found
            ? 'no media element found'
            : ''
    if (now && now !== saidCab) post({ speakers: (saidCab = now) })
  }
  if (cab) {
    setInterval(() => {
      cab.tick()
      tellCab()
    }, 250)
  }
  // How loud it is through the speakers, each ear (and the limiter's cut), for the live test (spotify.rs).
  window.westplaySpeakerLevel = () => cab?.level() ?? null
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
  // A page loaded anew: the host tells it the cab again.
  addEventListener('DOMContentLoaded', () => post({ ready: true }))

  // Spotify's own "Log in" (and "Sign up"), should the page ever show: its sign-in, on the desktop.
  const SIGN_IN = 'a[href*="accounts.spotify.com"], [data-testid="login-button"], [data-testid="signup-button"]'
  addEventListener(
    'click',
    (event) => {
      if (!event.isTrusted || !event.target?.closest?.(SIGN_IN)) return
      event.preventDefault()
      event.stopPropagation()
      post({ signIn: true })
    },
    true,
  )

  // Signed in (its account button on the page) or not ("Log in" there), and playing here (its element).
  // Neither yet, the page is still loading: nothing's said until it knows.
  const state = () => {
    const signedIn = document.querySelector('[data-testid="user-widget-link"]')
      ? true
      : document.querySelector('[data-testid="login-button"]')
        ? false
        : null
    return signedIn === null
      ? null
      : {
          signedIn,
          playing: playingLocally(),
        }
  }
  let told = ''
  const tellState = () => {
    const known = state()
    if (!known) return
    const now = JSON.stringify(known)
    if (now === told) return
    told = now
    post(JSON.parse(now))
  }
  setInterval(tellState, 500)
  // For the live test (spotify.rs): the element playing, its volume and time.
  window.westplayPlaying = () => {
    const element = playingNow()
    return element ? { paused: element.paused, volume: element.volume, time: element.currentTime, here: here() } : null
  }
})()
