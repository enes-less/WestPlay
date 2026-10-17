import { useEffect, useState } from 'react'
import { type HostStatus, appleMusicCheck, spotifyCheck, youtubeCheck } from './useHostStatus'

/**
 * Whether each service is signed in, as the companion last knew: shown at once when it opens, kept up to date
 * by every status that says (a sign-in or out here, or the service run in the cab). Only whether, nothing
 * else of the account, in the companion's own storage (gone with WestPlay when it's uninstalled). A service
 * never known is asked once, as the companion opens: run hidden to say.
 */
export interface SignedIn {
  youtube: boolean | null
  spotify: boolean | null
  appleMusic: boolean | null
}

export type Service = keyof SignedIn

const SERVICES: Service[] = ['youtube', 'spotify', 'appleMusic']

const KEY = 'westplay.companion.signedIn'

function kept(): SignedIn {
  let value: Partial<Record<Service, unknown>> | null = null
  try {
    value = JSON.parse(localStorage.getItem(KEY) ?? 'null') as Partial<Record<Service, unknown>> | null
  } catch {
    // Nothing kept: each is asked.
  }
  const known = (service: Service) => (typeof value?.[service] === 'boolean' ? (value[service] as boolean) : null)
  return { youtube: known('youtube'), spotify: known('spotify'), appleMusic: known('appleMusic') }
}

function keep(value: SignedIn) {
  try {
    localStorage.setItem(KEY, JSON.stringify(value))
  } catch {
    // Not kept: known again next time a service says.
  }
}

/** What's known now: the status's, else what was kept (saved again whenever a status says). */
export function useSignedIn(status: HostStatus | null): SignedIn {
  const [known] = useState(kept)
  const now: SignedIn = {
    youtube: status?.youtubeSignedIn ?? null,
    spotify: status?.spotifySignedIn ?? null,
    appleMusic: status?.appleMusicSignedIn ?? null,
  }
  useEffect(() => {
    if (SERVICES.every((service) => now[service] === null)) return
    const before = kept()
    keep({
      youtube: now.youtube ?? before.youtube,
      spotify: now.spotify ?? before.spotify,
      appleMusic: now.appleMusic ?? before.appleMusic,
    })
    // Keyed by the three answers (`now` itself is new at every render).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [now.youtube, now.spotify, now.appleMusic])
  return {
    youtube: now.youtube ?? known.youtube,
    spotify: now.spotify ?? known.spotify,
    appleMusic: now.appleMusic ?? known.appleMusic,
  }
}

/** As the companion opens: each service never known yet is run, hidden, to say. */
export function useAskOnce(): void {
  useEffect(() => {
    const known = kept()
    if (known.youtube === null) youtubeCheck().catch(() => {})
    if (known.spotify === null) spotifyCheck().catch(() => {})
    if (known.appleMusic === null) appleMusicCheck().catch(() => {})
  }, [])
}
