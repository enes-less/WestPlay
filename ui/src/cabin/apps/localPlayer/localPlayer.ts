import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'

type LocalPlayerState = {
  url?: string
  time: number
  setUrl(url: string): void
  setTime(time: number): void
}

export const useLocalPlayer = create<LocalPlayerState>()(
  persist(
    (set) => ({
      time: 0,
      setUrl: (url) => {
        set({ url })
      },
      setTime: (time) => {
        set({ time })
      }
    }),
    {
      name: 'westplay.localPlayer',
      version: 1,
      storage: createJSONStorage(() => localStorage),
    },
  ),
)
