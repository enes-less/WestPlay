import { useState } from 'react'
import { useText } from '../../state/text'
import { PersonGlyph } from './glyphs'
import { Cover } from './parts'
import { logIn, logOut, signUp, useAccount, useSpotify } from './spotify'

/** The account's pieces of the Spotify app: signing in, up and out, and Spotify's preview while signed out. */

/** On top, at the end: signed out, Spotify's Sign up and its white Log in; signed in, the account's button,
 * its menu holding Log out. Nothing while the web player hasn't said yet. */
export function AccountButtons() {
  const text = useText()
  const account = useAccount()
  const [menu, setMenu] = useState(false)
  if (account === 'unknown') return null
  if (account === 'out') {
    return (
      <>
        <button type="button" className="sp-sign-up" onClick={signUp}>
          {text.spotify.signUp}
        </button>
        <button type="button" className="sp-pill white" onClick={logIn}>
          {text.spotify.logIn}
        </button>
      </>
    )
  }
  return (
    <div className="sp-account">
      <button
        type="button"
        className="sp-round sp-me"
        aria-label={text.spotify.account}
        aria-expanded={menu}
        onClick={() => setMenu(!menu)}
      >
        <PersonGlyph />
      </button>
      {menu && (
        <div className="sp-menu" role="menu">
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setMenu(false)
              logOut()
            }}
          >
            {text.spotify.logOut}
          </button>
        </div>
      )}
    </div>
  )
}

/**
 * At the foot while signed out, where the mini player is signed in: Spotify's banner for its preview, Sign up
 * free; or, while Spotify's sign-in is open on the PC, where to finish it.
 */
export function PreviewBanner({ aside }: { aside: boolean }) {
  const text = useText()
  const account = useAccount()
  const signingIn = useSpotify((state) => state.signingIn)
  if (signingIn) {
    return (
      <p className={aside ? 'sp-banner sp-signing-in away' : 'sp-banner sp-signing-in'} role="status">
        <PersonGlyph />
        {text.spotify.finishOnPc}
      </p>
    )
  }
  if (account !== 'out') return null
  return (
    <div className={aside ? 'sp-banner away' : 'sp-banner'}>
      <span className="sp-banner-text">
        <b>{text.spotify.previewTitle}</b>
        <span>{text.spotify.previewText}</span>
      </span>
      <button type="button" className="sp-pill white" onClick={signUp}>
        {text.spotify.signUpFree}
      </button>
    </div>
  )
}

/** Spotify's prompt over the app, as its web player's when signed out: what was to play beside it, Sign up
 * free, and Log in for an account there is. */
export function LogInPrompt() {
  const text = useText()
  const prompt = useSpotify((state) => state.prompt)
  if (!prompt) return null
  const words = text.spotify.prompts[prompt.reason]
  const close = () => useSpotify.setState({ prompt: null })
  return (
    <div className="sp-prompt-veil" onClick={close}>
      <div
        className={prompt.cover ? 'sp-prompt' : 'sp-prompt small'}
        role="dialog"
        aria-label={words.title}
        onClick={(event) => event.stopPropagation()}
      >
        {prompt.cover && <Cover item={prompt.cover} className="sp-prompt-cover" />}
        <div className="sp-prompt-text">
          <h2>{words.title}</h2>
          {words.text && <p>{words.text}</p>}
          <button type="button" className="sp-pill green" onClick={signUp}>
            {text.spotify.signUpFree}
          </button>
          <p className="sp-prompt-log-in">
            {text.spotify.haveAccount}{' '}
            <button type="button" onClick={logIn}>
              {text.spotify.logIn}
            </button>
          </p>
          <button type="button" className="sp-prompt-close" onClick={close}>
            {text.spotify.notNow}
          </button>
        </div>
      </div>
    </div>
  )
}
