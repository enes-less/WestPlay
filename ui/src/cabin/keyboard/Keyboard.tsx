import { type CSSProperties, useEffect, useRef, useState } from 'react'
import { useLive } from '../state/store'
import { useText } from '../state/text'
import { DeleteGlyph, GlobeGlyph, HideGlyph, KeysGlyph, ShiftGlyph } from './glyphs'
import { capital, type Key, keyRows, type ReturnKind } from './layouts'
import { useKeyboard } from './store'
import './keyboard.css'

/** Holding delete: how long before it repeats, and how often then, ms. */
const REPEAT_AFTER_MS = 450
const REPEAT_EVERY_MS = 90

/**
 * The on-screen keyboard, iOS 26 style: a glass panel rising from the bottom of the CarPlay area while a
 * field is selected (store.ts), its keys rounded and translucent. A letter shows in a bubble above its key
 * while pressed and is typed when let go, so a slip can slide on to the right key first; shift and delete
 * act at once, delete repeating while held. Driven by the pointer (the interaction key's mouse).
 *
 * The player's own keyboard types into the field too while the truck stands still (the game plugin's
 * `typing`): a chip on top says so (or that it's paused while driving), and each key it types lights up here.
 */
export function Keyboard() {
  const field = useKeyboard((state) => state.field)
  const keys = useKeyboard((state) => state.keys)
  const physical = useKeyboard((state) => state.physical)
  const flash = useKeyboard((state) => state.flash)
  const moving = useLive((state) => (state.telemetry ? Math.abs(state.telemetry.speed) * 3.6 > 1 : false))
  const text = useText()
  // The return key keeps its name while the keyboard goes away.
  const [returnKind, setReturnKind] = useState<ReturnKind>('done')
  if (field && field.returnKind !== returnKind) setReturnKind(field.returnKind)

  // The key held down (row:index), while it's held over its key.
  const [held, setHeld] = useState<string | null>(null)
  // The pointer went down on the keyboard and is still down: it may slide from key to key.
  const down = useRef(false)
  const repeat = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const stopRepeat = () => clearTimeout(repeat.current)
  useEffect(() => {
    const letGo = () => {
      down.current = false
      setHeld(null)
      stopRepeat()
    }
    window.addEventListener('pointerup', letGo)
    window.addEventListener('pointercancel', letGo)
    return () => {
      window.removeEventListener('pointerup', letGo)
      window.removeEventListener('pointercancel', letGo)
      stopRepeat()
    }
  }, [])
  // Put away with a key held: it's let go.
  if (!field && held) setHeld(null)

  const press = (key: Key) => useKeyboard.getState().press(key)
  const startDeleting = () => {
    press({ kind: 'delete' })
    const again = (wait: number) => {
      repeat.current = setTimeout(() => {
        press({ kind: 'delete' })
        again(REPEAT_EVERY_MS)
      }, wait)
    }
    again(REPEAT_AFTER_MS)
  }

  // The key the player's own keyboard last typed, if it shows (in either case; space for a space).
  const flashed = (key: Key) =>
    flash !== null &&
    ((key.kind === 'char' && (key.char === flash.char || capital(key.char, keys.language) === flash.char)) ||
      (key.kind === 'space' && flash.char === ' '))
  const chip =
    physical === 'typing'
      ? text.keyboard.typingWithKeyboard
      : physical === 'paused'
        ? moving
          ? text.keyboard.pausedWhileDriving
          : text.keyboard.selectToType
        : null

  const rows = keyRows(keys.language, keys.layer)
  // Keys are as wide as the top row's: the rest of the rows centre under it.
  const style = { '--across': rows[0].length } as CSSProperties
  return (
    <div className={`keyboard ${keys.layer}${field ? ' shown' : ''}`} style={style} aria-hidden={!field} inert={!field}>
      {chip && (
        <div className={physical === 'typing' ? 'keyboard-chip typing' : 'keyboard-chip'} role="status">
          <KeysGlyph />
          {chip}
        </div>
      )}
      {rows.map((row, r) => (
        <div key={r} className={r === rows.length - 1 ? 'keyboard-row bottom' : 'keyboard-row'}>
          {row.map((key, i) => {
            const id = `${r}:${i}`
            // Shift and delete act as they go down; every other key when let go over it.
            const immediate = key.kind === 'shift' || key.kind === 'delete'
            return (
              <button
                key={id}
                type="button"
                tabIndex={-1}
                className={keyClass(key, keys.shift !== 'off')}
                aria-label={keyName(key, text)}
                onPointerDown={(event) => {
                  if (event.button !== 0) return
                  down.current = true
                  if (key.kind === 'shift') press(key)
                  else if (key.kind === 'delete') startDeleting()
                  if (!immediate) setHeld(id)
                }}
                onPointerEnter={(event) => {
                  if (!immediate && down.current && (event.buttons & 1) !== 0) setHeld(id)
                }}
                onPointerLeave={() => {
                  if (held === id) setHeld(null)
                  if (key.kind === 'delete') stopRepeat()
                }}
                onPointerUp={() => {
                  if (held === id) press(key)
                }}
              >
                <KeyFace
                  face={key}
                  shiftOn={keys.layer === 'letters' && keys.shift !== 'off'}
                  language={keys.language}
                />
                {key.kind === 'char' && held === id && (
                  <span className="key-bubble" aria-hidden>
                    <KeyFace
                      face={key}
                      shiftOn={keys.layer === 'letters' && keys.shift !== 'off'}
                      language={keys.language}
                    />
                  </span>
                )}
                {flash && flashed(key) && (
                  // Remade for every key typed, so its light (and bubble) plays again.
                  <span key={flash.id} className="key-flash" aria-hidden>
                    {key.kind === 'char' && (
                      <span className="key-bubble">
                        <KeyFace face={key} shiftOn={flash.char !== key.char} language={keys.language} />
                      </span>
                    )}
                  </span>
                )}
                {key.kind === 'shift' && <ShiftGlyph shift={keys.shift} />}
                {key.kind === 'delete' && <DeleteGlyph />}
                {key.kind === 'globe' && <GlobeGlyph />}
                {key.kind === 'hide' && <HideGlyph />}
                {key.kind === 'space' && text.keyboard.space}
                {key.kind === 'return' && text.keyboard.returnKey[returnKind]}
                {key.kind === 'layer' && LAYER_LABELS[key.to]}
              </button>
            )
          })}
        </div>
      ))}
    </div>
  )
}

/** What the layer keys say: the layer they bring. */
const LAYER_LABELS = { letters: 'ABC', numbers: '123', symbols: '#+=' } as const

/** A character key's face: its character, in capitals while shift is on. */
function KeyFace({ face, shiftOn, language }: { face: Key; shiftOn: boolean; language: 'en' | 'tr' }) {
  if (face.kind !== 'char') return null
  return <>{shiftOn ? capital(face.char, language) : face.char}</>
}

function keyClass(key: Key, shiftOn: boolean): string {
  switch (key.kind) {
    case 'char':
      return 'key'
    case 'space':
      return 'key space'
    case 'return':
      return 'key return'
    case 'shift':
      return shiftOn ? 'key fn wide shift on' : 'key fn wide shift'
    case 'delete':
      return 'key fn wide'
    case 'layer':
      return 'key fn wide label'
    case 'globe':
    case 'hide':
      return 'key fn'
  }
}

/** A key's name for screen readers, where its face is a symbol. */
function keyName(key: Key, text: ReturnType<typeof useText>): string | undefined {
  switch (key.kind) {
    case 'shift':
      return text.keyboard.shift
    case 'delete':
      return text.keyboard.delete
    case 'globe':
      return text.keyboard.nextKeyboard
    case 'hide':
      return text.keyboard.hide
    default:
      return undefined
  }
}
