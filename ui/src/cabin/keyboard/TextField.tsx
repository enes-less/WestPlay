import { type ReactNode, useEffect, useLayoutEffect, useMemo, useRef } from 'react'
import { useLive } from '../state/store'
import { languageOf } from '../state/strings'
import { useText } from '../state/text'
import type { ReturnKind } from './layouts'
import { type Typing, useKeyboard } from './store'

interface TextFieldProps {
  /** Unique among the fields on screen. */
  id: string
  value: string
  onChange(value: string): void
  /** The return key (on-screen or the player's own). */
  onSubmit?(): void
  returnKind?: ReturnKind
  /** What the field is for, shown while it's empty. */
  hint: string
  /** Its name for screen readers. */
  label: string
  /** A symbol before the text (search's magnifier). */
  icon?: ReactNode
  /** Selected as it appears, the keyboard coming up with it. */
  selectOnShow?: boolean
}

/**
 * A field the keyboard types into. Not an `<input>`: the page never has the PC's keyboard (the game does),
 * so WestPlay draws its own field, caret and all, and the keyboard (store.ts) writes into it. Pressing it
 * selects it and brings the keyboard up; it lets go when it leaves the screen.
 */
export function TextField({
  id,
  value,
  onChange,
  onSubmit,
  returnKind = 'done',
  hint,
  label,
  icon,
  selectOnShow = false,
}: TextFieldProps) {
  const text = useText()
  const typing = useKeyboard((state) => state.field?.id === id)
  const language = languageOf(useLive((state) => state.gameLanguage))

  // The newest props, for the keyboard to call; and the text as last written, so keys pressed faster than
  // the page redraws (the player's own keyboard) each build on the one before.
  const latest = useRef({ onChange, onSubmit })
  const current = useRef(value)
  useLayoutEffect(() => {
    latest.current = { onChange, onSubmit }
    current.current = value
  })
  const field = useMemo<Typing>(
    () => ({
      id,
      returnKind,
      read: () => current.current,
      write: (next) => {
        current.current = next
        latest.current.onChange(next)
      },
      submit: () => latest.current.onSubmit?.(),
    }),
    [id, returnKind],
  )
  const select = () => useKeyboard.getState().attach(field, language)

  useEffect(() => {
    if (selectOnShow) useKeyboard.getState().attach(field, language)
    return () => useKeyboard.getState().detach(field.id)
  }, [field, selectOnShow, language])

  // A long text shows its end, where the typing is.
  const shown = useRef<HTMLSpanElement>(null)
  useLayoutEffect(() => {
    if (shown.current) shown.current.scrollLeft = shown.current.scrollWidth
  }, [value, typing])

  return (
    <div
      className={typing ? 'text-field typing' : 'text-field'}
      role="textbox"
      aria-label={label}
      onPointerDown={(event) => {
        if (event.button === 0) select()
      }}
    >
      {icon}
      <span ref={shown} className="text-field-text">
        {value}
        {typing && <span className="text-field-caret" aria-hidden />}
        {!value && <span className="text-field-hint">{hint}</span>}
      </span>
      {value && (
        <button
          type="button"
          tabIndex={-1}
          className="text-field-clear"
          aria-label={text.keyboard.clear}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={() => {
            current.current = ''
            onChange('')
          }}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="m7 7 10 10M17 7 7 17" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
          </svg>
        </button>
      )}
    </div>
  )
}
