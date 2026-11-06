import { useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Button, Group, Row, Tick } from '../room/parts'
import type { CompanionWords } from './words'

/** One map to prepare again: its folder (what the host asks by), the profiles playing it and what it's made of. */
export interface MapChoice {
  folder: string
  title: string
  detail: string
}

/**
 * Prepare Maps Again, with more than one map: a sheet over the room listing each map once (profiles playing the
 * same map share it), every one ticked; the player unticks those to keep as they are (owner 2026-11-06: a player
 * with many profiles' maps picks which to draw again), and Prepare asks for the rest.
 */
export function PrepareMapsSheet({
  cw,
  maps,
  prepare,
  close,
}: {
  cw: CompanionWords
  maps: MapChoice[]
  prepare(folders: string[]): void
  close(): void
}) {
  const w = cw.map
  const [chosen, setChosen] = useState(() => new Set(maps.map((map) => map.folder)))
  const toggle = (folder: string) =>
    setChosen((before) => {
      const after = new Set(before)
      if (!after.delete(folder)) after.add(folder)
      return after
    })
  // Whether the list holds more than it shows: only then does it fade at its foot.
  const list = useRef<HTMLDivElement>(null)
  const [more, setMore] = useState(false)
  useLayoutEffect(() => {
    const element = list.current
    if (!element) return
    const look = () => setMore(element.scrollHeight - element.scrollTop - element.clientHeight > 1)
    look()
    element.addEventListener('scroll', look)
    const resized = new ResizeObserver(look)
    resized.observe(element)
    return () => {
      element.removeEventListener('scroll', look)
      resized.disconnect()
    }
  }, [])
  const root = document.querySelector('.cp')
  if (!root) return null
  return createPortal(
    <div className="cp-veil" onClick={close}>
      <section
        className="cp-sheet cp-sheet-tall"
        role="dialog"
        aria-label={w.prepareAgain}
        onClick={(event) => event.stopPropagation()}
      >
        <h2>{w.prepareAgain}</h2>
        <p>{w.pickText}</p>
        {/* The maps with room to see them; more than the window holds scroll, fading at the foot. */}
        <div ref={list} className="cp-sheet-list cp-sheet-maps" data-more={more || undefined}>
          <Group place={0}>
            {maps.map((map) => (
              <Row
                key={map.folder}
                title={map.title}
                detail={map.detail}
                accessory={chosen.has(map.folder) ? <Tick key="chosen" /> : undefined}
                onClick={() => toggle(map.folder)}
              />
            ))}
          </Group>
        </div>
        <div className="cp-sheet-actions">
          <Button glass onClick={close}>
            {cw.uninstall.cancel}
          </Button>
          <Button
            onClick={() => prepare(maps.filter((map) => chosen.has(map.folder)).map((map) => map.folder))}
            disabled={chosen.size === 0}
          >
            {w.prepare}
          </Button>
        </div>
      </section>
    </div>,
    root,
  )
}
