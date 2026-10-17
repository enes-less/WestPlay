import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { Companion } from './Companion'

// No right-click menu, as in an app (the browser's Back, Reload and Inspect have no place here); a text field
// keeps its own, for copy and paste.
document.addEventListener('contextmenu', (event) => {
  const target = event.target instanceof Element ? event.target : null
  if (!target?.closest('input, textarea, [contenteditable="true"]')) event.preventDefault()
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Companion />
  </StrictMode>,
)
