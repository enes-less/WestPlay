import { useCallback, useEffect, useState } from 'react'
import { COMPANION_FILES, type Customizations, fileUrl } from '../shared/customize'
import { call, IN_APP, on } from './bridge'

/** What adding did: the wallpapers' or the icon set's names as they were kept, and the files left out, with
 * why. Mirrors `westplay_host::customize::Added`. */
export interface Added {
  added: string[]
  problems: { file: string; reason: AddProblem }[]
}

export type AddProblem = 'notAnImage' | 'tooLarge' | 'unknownName' | 'twice' | 'unreadable'

/** Where the companion finds the player's files: the app's address for them; in the preview, its own files. */
export const FILES_BASE = IN_APP ? COMPANION_FILES : '/'

/** A file of the player's, at the companion's address for it. */
export function customFile(path: string): string {
  return fileUrl(FILES_BASE, path)
}

/** The player's wallpapers and icon sets, as last listed, and how to list them again (after a change). */
export function useCustomizations(): { custom: Customizations | null; refresh(): void } {
  const [custom, setCustom] = useState<Customizations | null>(null)
  const refresh = useCallback(() => {
    call<Customizations>('customize_list').then(setCustom, (error: unknown) =>
      console.error('The wallpapers and icon sets could not be listed', error),
    )
  }, [])
  useEffect(refresh, [refresh])
  return { custom, refresh }
}

/** Calls `handler` when a picker has closed with a choice and adding it begins (copying and checking the files,
 * which a large photo or .zip makes take a while). Returns how to stop. */
export function onAdding(handler: () => void): () => void {
  return on('customize-adding', handler)
}

/** Asks for images (Windows' picker, titled `title`, its filter `kinds`), or (`folder`) a folder of them, and adds
 * them, each with its other look beside it (`silk.jpg`, `silk.dark.jpg`); `null`: cancelled. */
export function addWallpapers(folder: boolean, title: string, kinds: string): Promise<Added | null> {
  return call('add_wallpapers', { folder, title, kinds })
}

/** Asks for an icon set, a folder or (`zip`) a .zip, and adds it; `null`: cancelled. */
export function addIconSet(zip: boolean, title: string, kinds: string): Promise<Added | null> {
  return call('add_icon_set', { zip, title, kinds })
}

export function removeWallpaper(id: string): Promise<void> {
  return call('remove_wallpaper', { id })
}

export function removeIconSet(name: string): Promise<void> {
  return call('remove_icon_set', { name })
}
