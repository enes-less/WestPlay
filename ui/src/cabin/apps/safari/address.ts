/**
 * Safari's address field: what's typed made a web address, or a search with the player's search engine (Settings ›
 * Safari › Search Engine, as an iPad's); and an address shown as Safari shows it (the site's name alone, or the
 * search's words on its engine's results).
 */

/** The search engines an iPad's Safari offers (Settings › Apps › Safari › Search Engine), in its order. */
export const SEARCH_ENGINES = ['google', 'yahoo', 'bing', 'duckDuckGo', 'ecosia'] as const

export type SearchEngine = (typeof SEARCH_ENGINES)[number]

/** Google, as an iPad's Safari starts. */
export const DEFAULT_ENGINE: SearchEngine = 'google'

/** Each engine's name (its own, in every language) and where it searches: its results page, the words in `key`. */
const ENGINES: Record<SearchEngine, { name: string; results: string; key: string; hosts: string[] }> = {
  google: { name: 'Google', results: 'https://www.google.com/search', key: 'q', hosts: ['google.'] },
  yahoo: { name: 'Yahoo', results: 'https://search.yahoo.com/search', key: 'p', hosts: ['search.yahoo.'] },
  bing: { name: 'Bing', results: 'https://www.bing.com/search', key: 'q', hosts: ['bing.com'] },
  duckDuckGo: { name: 'DuckDuckGo', results: 'https://duckduckgo.com/', key: 'q', hosts: ['duckduckgo.com'] },
  ecosia: { name: 'Ecosia', results: 'https://www.ecosia.org/search', key: 'q', hosts: ['ecosia.org'] },
}

export function engineName(engine: SearchEngine): string {
  return ENGINES[engine].name
}

export function isSearchEngine(value: unknown): value is SearchEngine {
  return typeof value === 'string' && (SEARCH_ENGINES as readonly string[]).includes(value)
}

/** The results of a search for `words` with `engine`. */
export function searchAddress(words: string, engine: SearchEngine): string {
  const { results, key } = ENGINES[engine]
  return `${results}?${key}=${encodeURIComponent(words.trim()).replace(/%20/g, '+')}`
}

/** A name that reads as a site's: letters, digits and hyphens in dotted parts, the last of them (its top-level
 * domain) letters; or `localhost`, or an IPv4 address; a port after it allowed. */
const HOST =
  /^(localhost|(\d{1,3}\.){3}\d{1,3}|([a-z0-9¡-￿](?:[a-z0-9¡-￿-]*[a-z0-9¡-￿])?\.)+[a-z¡-￿]{2,63})(:\d{1,5})?$/i

/**
 * What the player typed into the address field, as the address to load: a web address as it is; something that
 * reads as one without its `https://` (a site's name, a path after it) with it; anything else (words, another
 * kind of address: `javascript:`, `file:`) searched for. `null` for nothing typed.
 */
export function addressFor(typed: string, engine: SearchEngine): string | null {
  const text = typed.trim()
  if (!text) return null
  if (/^https?:\/\/\S+$/i.test(text)) {
    try {
      return new URL(text).href
    } catch {
      return searchAddress(text, engine)
    }
  }
  if (!/\s/.test(text)) {
    const host = text.split(/[/?#]/, 1)[0]
    if (HOST.test(host)) {
      try {
        return new URL(`https://${text}`).href
      } catch {
        // Searched for, below.
      }
    }
  }
  return searchAddress(text, engine)
}

/** How the address field shows a page's address while it's not typed into: the site's name (without `www.`),
 * whether it's a secure page, and for a search engine's results the search's words. */
export interface Shown {
  text: string
  secure: boolean
  search: boolean
}

export function shownAddress(url: string): Shown {
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    return { text: url, secure: false, search: false }
  }
  const secure = parsed.protocol === 'https:'
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return { text: url, secure: false, search: false }
  const host = parsed.hostname.replace(/^www\./, '')
  for (const engine of Object.values(ENGINES)) {
    if (!engine.hosts.some((name) => parsed.hostname.includes(name))) continue
    const words = parsed.searchParams.get(engine.key)
    if (words && (parsed.pathname === new URL(engine.results).pathname || parsed.pathname === '/search')) {
      return { text: words, secure, search: true }
    }
  }
  return { text: host, secure, search: false }
}

/** A page's site, for the history's and the Start Page's matching (`example.com`, without `www.`). */
export function siteOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '')
  } catch {
    return ''
  }
}
