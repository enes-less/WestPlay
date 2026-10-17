import '@fontsource-variable/instrument-sans'
import '@fontsource-variable/inter'
// The YouTube app's type, as YouTube's own.
import '@fontsource-variable/roboto'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { CabinApp } from './CabinApp'
import { installDemo } from './demo'
import { Recovery } from './shell/Recovery'
import { useCabin, useLive } from './state/store'
import './shell/shell.css'
import './shell/apple.css'

// Development builds expose the stores for console debugging and browser previews (`westplay.cabin`).
if (import.meta.env.DEV) {
  Object.assign(window, { westplay: { cabin: useCabin, live: useLive } })
}

installDemo()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Recovery>
      <CabinApp />
    </Recovery>
  </StrictMode>,
)
