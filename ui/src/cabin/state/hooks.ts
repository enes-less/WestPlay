import { useEffect, useMemo } from 'react'
import { create } from 'zustand'
import { fileUrl } from '../../shared/customize'
import { type AppIconRole, appIcon } from '../../shared/icons'
import { findWallpaper, pickVariant } from '../wallpapers'
import { type Appearance, resolveAppearance } from './appearance'
import { animateAppearance } from './launch'
import { useLooks, useWallpapers } from './looks'
import { type OemStripId, resolveOemStrip } from './oem'
import { type ScreenProfile, screenProfile } from './screen'
import { useCabin, useLive } from './store'

export interface ResolvedLook {
  /** The appearance the UI uses (follows the wallpaper variant actually shown). */
  appearance: Appearance
  wallpaperSrc: string
}

/** The look on the screen, which follows the wanted one (`useLookChanges`); `null` until the shell is up. */
const useShownLook = create<{ look: ResolvedLook | null }>()(() => ({ look: null }))

/** Appearance and wallpaper image, from the appearance mode, the truck (time, headlights) and the wallpaper. */
function useWantedLook(): ResolvedLook {
  const mode = useCabin((state) => state.appearanceMode)
  const wallpaperId = useCabin((state) => state.wallpaperId)
  const wallpapers = useWallpapers()
  // Select primitives: telemetry arrives ~20 times a second, but these only change every game minute.
  const gameTime = useLive((state) => state.telemetry?.gameTime ?? null)
  const lowBeam = useLive((state) => state.telemetry?.lowBeam ?? false)

  return useMemo(() => {
    const wanted = resolveAppearance(mode, { gameTime, lowBeam })
    const { appearance, src } = pickVariant(findWallpaper(wallpaperId, wallpapers), wanted)
    return { appearance, wallpaperSrc: src }
  }, [mode, wallpaperId, wallpapers, gameTime, lowBeam])
}

/** The appearance and wallpaper image on the screen: everything draws in this one, so it changes as one. */
export function useLook(): ResolvedLook {
  const wanted = useWantedLook()
  return useShownLook((state) => state.look) ?? wanted
}

/**
 * For the shell alone: brings the screen to the wanted look. A new appearance crossfades the whole screen
 * (state/launch.ts); a new wallpaper in the same appearance fades in on its own (Wallpaper.tsx).
 */
export function useLookChanges(): void {
  const wanted = useWantedLook()
  useEffect(() => {
    const shown = useShownLook.getState().look
    if (shown?.appearance === wanted.appearance && shown.wallpaperSrc === wanted.wallpaperSrc) return
    const show = () => useShownLook.setState({ look: wanted })
    if (shown && shown.appearance !== wanted.appearance) animateAppearance(show)
    else show()
  }, [wanted])
}

/**
 * An app's icon, in the look the icon style asks for (`auto`: the screen's own): the chosen icon set's, or
 * WestPlay's own where the set has none for that look.
 */
export function useAppIcon(role: AppIconRole): string {
  const { appearance } = useLook()
  return useLooks((state) => {
    const look = state.iconStyle === 'auto' ? appearance : state.iconStyle
    const icon = state.custom.iconSets.find((set) => set.name === state.iconSet)?.icons[role]
    const path = icon?.[look]
    return path ? fileUrl(state.base, path) : appIcon(role, look)
  })
}

/** How WestPlay fits the screen the game draws it on (screen layout + truck model). */
export function useScreenProfile(): ScreenProfile {
  const layout = useLive((state) => state.screenLayout)
  const truck = useLive((state) => state.truck)
  return useMemo(() => screenProfile(layout, truck), [layout, truck])
}

/** The OEM strip to draw around CarPlay, if any. */
export function useOemStrip(): OemStripId | null {
  const mode = useCabin((state) => state.oemMode)
  return resolveOemStrip(mode, useScreenProfile().oemStrip)
}
