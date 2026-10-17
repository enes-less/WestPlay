import { create } from 'zustand'

/** The Settings app's pages: its list, and the page each row opens. */
export type SettingsPage = 'list' | 'appearance' | 'wallpaper' | 'dashboard' | 'nowPlaying'

interface SettingsState {
  /** The page shown; kept while the app is closed, as iOS's Settings comes back where it was. */
  page: SettingsPage
  /** Which way the last page change went: a page slides in from the right, the list back from the left
   * (`none`: the app has just opened, and the page grows with its window instead). */
  went: 'none' | 'in' | 'back'
  open(page: SettingsPage): void
  back(): void
}

export const useSettings = create<SettingsState>()((set) => ({
  page: 'list',
  went: 'none',
  open: (page) => set({ page, went: 'in' }),
  back: () => set({ page: 'list', went: 'back' }),
}))
