import type { AppId } from '../state/apps'
import type { Language } from '../state/text'

/**
 * Launch copies (owner 2026-10-21): a saved copy of each heavy app as it first shows when opened, its own
 * skeletons, kept in the page's storage on disk. Opening one, its window grows out of the icon showing that
 * light, static copy; the real app mounts once the window has opened, fetches its data, and takes the copy's
 * place (its first frame is the copy's look, so nothing changes to the eye), its content filling in as ever.
 *
 * Only an app's first opening since the page loaded goes through its copy: that one built the app cold and
 * froze; later openings were always smooth, and open the app straight away (owner 2026-10-21).
 *
 * A copy is the real app's first frame at its last opening (apps/AppScreen.tsx), as the player saw it:
 * signed in or not, in their language and appearance (each kept apart). A copy built apart, knowing nothing
 * of the account, showed the signed-out app (owner 2026-10-21). The first opening of an app, with no copy
 * yet, builds it at once, as before.
 */

/** The apps opened through a launch copy: those whose building held up their window's opening. */
export const COPIED: readonly AppId[] = ['music', 'spotify', 'youtubeMusic']

export function isCopied(app: AppId): boolean {
  return COPIED.includes(app)
}

/** The copies' layout version: a change to how they're made or shown leaves the old ones unread (1: built
 * apart, signed out; 2: saved at any opening, with its content). */
const VERSION = 3

// The copies of an older layout, gone (they'd only take room).
try {
  for (const key of Object.keys(localStorage)) {
    if (key.startsWith('westplay.launchCopy.') && !key.startsWith(`westplay.launchCopy.v${VERSION}.`)) {
      localStorage.removeItem(key)
    }
  }
} catch {
  // Storage blocked: nothing kept to clear.
}

/** The appearance the screen shows now, as the shell wears it. */
function appearanceNow(): string {
  return document.querySelector('.shell')?.classList.contains('appearance-light') ? 'light' : 'dark'
}

/** Where `app`'s copy for `language` and the appearance now is kept. */
export function copyKey(app: AppId, language: Language): string {
  return `westplay.launchCopy.v${VERSION}.${app}.${language}.${appearanceNow()}`
}

/** `app`'s saved copy (its window's markup) for `language` and the appearance now, if there is one. */
export function savedCopy(app: AppId, language: Language): string | null {
  try {
    return localStorage.getItem(copyKey(app, language))
  } catch {
    // Storage blocked: opened without a copy, as before.
    return null
  }
}

/** The apps opened since the page loaded: opened again, straight away, without their copy. */
const opened = new Set<AppId>()

/** Whether `app` has been opened since the page loaded. */
export function openedBefore(app: AppId): boolean {
  return opened.has(app)
}

/** Notes `app` as opened since the page loaded. */
export function noteOpened(app: AppId): void {
  opened.add(app)
}

/** Saves `app`'s copy for `language` and the appearance now. */
export function saveCopy(app: AppId, language: Language, markup: string): void {
  try {
    localStorage.setItem(copyKey(app, language), markup)
  } catch {
    // Storage full or blocked: opened without a copy, as before.
  }
}
