import { useEffect, useState } from 'react'
import { call, on } from './bridge'

/** An update to WestPlay, found on its releases. Mirrors the app's `update::Offer`. */
export interface Offer {
  version: string
  /** What's new: the changelog's notes for it (Markdown, a list). */
  notes: string
  /** When it was released, `YYYY-MM-DD`. */
  date: string | null
}

/** How far the download is. */
export interface Progress {
  done: number
  total: number | null
}

/** The update found, if one is: as the app had it when asked, then as it finds one (every few hours). */
export function useUpdateOffer(): Offer | null {
  const [offer, setOffer] = useState<Offer | null>(null)
  useEffect(() => {
    let live = true
    call<Offer | null>('update_offer').then(
      (found) => live && found && setOffer(found),
      () => {},
    )
    const stop = on<Offer>('update-found', setOffer)
    return () => {
      live = false
      stop()
    }
  }, [])
  return offer
}

/** Looks for an update now. */
export function lookForUpdate(): Promise<Offer | null> {
  return call('look_for_update')
}

/** Installs the update found: comes back only if it couldn't (WestPlay quits for the installer). */
export function installUpdate(): Promise<void> {
  return call('install_update')
}

/** Follows the download of the update being installed. */
export function onUpdateProgress(handler: (progress: Progress) => void): () => void {
  return on<Progress>('update-progress', handler)
}

/** The notes' lines: the changelog's list items, without their Markdown. */
export function noteLines(notes: string): string[] {
  return notes
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.startsWith('- ') || line.startsWith('* '))
    .map((line) =>
      line
        .slice(2)
        .replace(/\*\*(.+?)\*\*/g, '$1')
        .replace(/`(.+?)`/g, '$1'),
    )
}
