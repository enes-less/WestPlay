import { useEffect, useMemo } from 'react'
import { create } from 'zustand'
import { CABIN_FILES, type Customizations, fileUrl, type IconStyle, NO_CUSTOMIZATIONS } from '../../shared/customize'
import { type AppIconRole, appIcon } from '../../shared/icons'
import { BUILT_IN_WALLPAPERS, customWallpaper, type Wallpaper } from '../wallpapers'
import { animateAppearance } from './launch'

/**
 * The player's own wallpapers and icon sets (the companion's Customize), and the icon set chosen: from the
 * host (`customize`, `settings`), or in the companion's demo from the companion itself (demo.ts), whose files
 * are at its own address.
 */
interface Looks {
  custom: Customizations
  iconSet: string | null
  iconStyle: IconStyle
  /** Where the files are. */
  base: string
}

export const useLooks = create<Looks>()(() => ({
  custom: NO_CUSTOMIZATIONS,
  iconSet: null,
  iconStyle: 'auto',
  base: CABIN_FILES,
}))

export function isIconStyle(style: unknown): style is IconStyle {
  return style === 'auto' || style === 'light' || style === 'dark'
}

const ROLES: AppIconRole[] = [
  'maps',
  'music',
  'spotify',
  'podcasts',
  'messages',
  'phone',
  'settings',
  'weather',
  'youtube',
  'youtubeMusic',
]

/** Every icon file an icon set (`null`: WestPlay's own) can show, in both looks. */
function iconFiles(state: Looks, set: string | null): string[] {
  const chosen = state.custom.iconSets.find((iconSet) => iconSet.name === set)
  return ROLES.flatMap((role) =>
    (['light', 'dark'] as const).map((look) => {
      const path = chosen?.icons[role]?.[look]
      return path ? fileUrl(state.base, path) : appIcon(role, look)
    }),
  )
}

/** Icons loaded and decoded, kept so they stay ready: a change of look shows them at once, inside its crossfade,
 * rather than popping in after it. */
const ready = new Map<string, Promise<void>>()

function prepare(files: string[]): Promise<void> {
  return Promise.all(
    files.map((file) => {
      let made = ready.get(file)
      if (!made) {
        const image = new Image()
        image.src = file
        made = image.decode().catch(() => {})
        ready.set(file, made)
      }
      return made
    }),
  ).then(() => {})
}

/** For the shell: the icons on show, ready in both looks, so the screen's Light/Dark crossfade takes them along. */
export function usePreparedIcons(): void {
  const files = useLooks((state) => iconFiles(state, state.iconSet).join('\n'))
  useEffect(() => void prepare(files.split('\n')), [files])
}

/** How long a change of icons waits for its new ones to be ready, before it shows them anyway. */
const READY_WAIT_MS = 800

/**
 * Changes the icon set or the icons' look as the screen changes its appearance: one crossfade of the whole
 * screen (launch.ts), once the new icons are ready.
 */
export function setIcons(change: Partial<Pick<Looks, 'iconSet' | 'iconStyle'>>): void {
  const now = useLooks.getState()
  const set = change.iconSet === undefined ? now.iconSet : change.iconSet
  const style = change.iconStyle ?? now.iconStyle
  const call = ++latest
  // The first, as the page loads: what it is, no crossfade.
  if (!settled) {
    settled = true
    useLooks.setState({ iconSet: set, iconStyle: style })
    return
  }
  if (set === now.iconSet && style === now.iconStyle) return
  const wait = new Promise((resolve) => setTimeout(resolve, READY_WAIT_MS))
  void Promise.race([prepare(iconFiles(now, set)), wait]).then(() => {
    // A newer change came meanwhile: it's the one shown.
    if (call === latest) animateAppearance(() => useLooks.setState({ iconSet: set, iconStyle: style }))
  })
}

/** The icons have been set once (the page's first settings). */
let settled = false
/** The latest change of icons asked for. */
let latest = 0

/** Every wallpaper there is to choose: WestPlay's own, then the player's. */
export function useWallpapers(): Wallpaper[] {
  const custom = useLooks((state) => state.custom.wallpapers)
  const base = useLooks((state) => state.base)
  return useMemo(() => [...BUILT_IN_WALLPAPERS, ...custom.map((w) => customWallpaper(w, base))], [custom, base])
}
