// WestPlay, in Apple Music's sign-in window (signin.rs): Apple's own web player, where signing in is a dialog of
// its own. The window is for signing in: the dialog opens at once (the page's own Sign In pressed), and once
// the page is signed in (MusicKit, the page's own player, says so) this says it, nothing more of the account.
;(() => {
  if (location.hostname !== 'music.apple.com' || window.top !== window) return
  const post = (message) => window.chrome?.webview?.postMessage(message)
  let opened = false
  const look = () => {
    let music = null
    try {
      music = window.MusicKit?.getInstance?.() ?? null
    } catch {
      // Not there yet.
    }
    if (music?.isAuthorized) {
      post({ signedIn: true })
      return
    }
    // The page's own Sign In, pressed once it's there (again, should the page load anew).
    const button = document.querySelector('button[data-testid="sign-in-button"]')
    if (button && !opened) {
      opened = true
      button.click()
    }
    setTimeout(look, 500)
  }
  look()
})()
