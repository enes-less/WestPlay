import { useEffect, useState } from 'react'
import { BUILT_IN_WALLPAPERS, customWallpaper, type Wallpaper } from '../../cabin/wallpapers'
import type { IconSet, IconStyle } from '../../shared/customize'
import { type AppIconRole, appIcon } from '../../shared/icons'
import {
  type Added,
  addIconSet,
  addWallpapers,
  customFile,
  FILES_BASE,
  onAdding,
  removeIconSet,
  removeWallpaper,
} from '../customize'
import { Checking, Group, Note, Panel, Row, Tick } from '../room/parts'
import { list } from '../onboarding/words'
import { useWords } from '../room/room'
import { GLOW, useScreenAt } from '../room/screen'
import type { Settings } from '../useSettings'
import type { PageProps } from './page'
import type { CompanionWords } from './words'

const STYLES: IconStyle[] = ['auto', 'light', 'dark']

/** What's being added: wallpapers (pictures or a folder of them), or an icon set (a folder or a .zip). */
type Adding = 'wallpapers' | 'wallpaperFolder' | 'folder' | 'zip'

/** The apps an icon set has icons for, in the order Home shows them, by their files' names. */
const ICON_FILES: [AppIconRole, string][] = [
  ['maps', 'maps'],
  ['music', 'music'],
  ['spotify', 'spotify'],
  ['podcasts', 'podcasts'],
  ['messages', 'messages'],
  ['phone', 'phone'],
  ['settings', 'settings'],
  ['weather', 'weather'],
  ['youtube', 'youtube'],
  ['youtubeMusic', 'youtube-music'],
]

/**
 * Customize: the player's own wallpapers and app icons. Wallpapers in a grid (WestPlay's, then theirs, and
 * one to add more), app icons as sets (WestPlay's own, then theirs, each showing its icons, and two ways to
 * add one). Choosing one shows it on the room's screen as on the truck's: the companion tells its screen
 * (Companion.tsx), the app tells the truck's (`look` settings). On the room's screen, Home.
 */
export function Customize({ cw, settings, custom, refreshCustom }: PageProps) {
  const w = cw.customize
  const words = useWords()
  // What the last add did, shown under the section it added to.
  const [report, setReport] = useState<{ section: 'wallpapers' | 'icons'; added: Added } | null>(null)
  // What's being added: its picker open, then (`adding`) its files being copied and checked, a spinner on it.
  const [working, setWorking] = useState<Adding | null>(null)
  const [adding, setAdding] = useState(false)
  useEffect(() => onAdding(() => setAdding(true)), [])
  useScreenAt('home', GLOW.home, null)
  const look = settings.settings?.look
  const choose = (change: Partial<Settings['look']>) => {
    const current = settings.settings
    if (current) void settings.update({ ...current, look: { ...current.look, ...change } })
  }

  const add = async (what: Adding, adding: () => Promise<Added | null>) => {
    if (working) return
    setWorking(what)
    setAdding(false)
    try {
      const added = await adding()
      if (added) {
        setReport({ section: what === 'wallpapers' || what === 'wallpaperFolder' ? 'wallpapers' : 'icons', added })
        refreshCustom()
      }
    } catch (error) {
      console.error('Adding failed', error)
    } finally {
      setWorking(null)
      setAdding(false)
    }
  }
  const spinning = (what: Adding) => working === what && adding
  const remove = async (removing: Promise<void>) => {
    setReport(null)
    await removing.catch((error: unknown) => console.error('Removing failed', error))
    refreshCustom()
  }

  // The look the icons show in: the room's screen is dark, so Auto shows them dark here.
  const shown = look?.iconStyle === 'light' ? 'light' : 'dark'
  const wallpapers = [...BUILT_IN_WALLPAPERS, ...(custom?.wallpapers ?? []).map((x) => customWallpaper(x, FILES_BASE))]
  const chosenWallpaper = look?.wallpaper ?? wallpapers[0]?.id
  const names = list(
    words,
    ICON_FILES.map(([, file]) => file),
  )

  return (
    <Panel place="page" title={w.title} sub={w.sub}>
      <Group title={w.wallpapers} place={4}>
        <div className="cz-wallpapers">
          {wallpapers.map((wallpaper) => (
            <WallpaperTile
              key={wallpaper.id}
              wallpaper={wallpaper}
              chosen={wallpaper.id === chosenWallpaper}
              pair={Boolean(
                wallpaper.variants.dark &&
                wallpaper.variants.light &&
                wallpaper.variants.dark !== wallpaper.variants.light,
              )}
              cw={cw}
              choose={() => choose({ wallpaper: wallpaper.id })}
              remove={wallpaper.id.startsWith('custom:') ? () => void remove(removeWallpaper(wallpaper.id)) : undefined}
            />
          ))}
          <button
            type="button"
            className="cz-add"
            disabled={working !== null}
            aria-busy={spinning('wallpapers')}
            onClick={() => void add('wallpapers', () => addWallpapers(false, w.pickWallpapers, w.images))}
          >
            {spinning('wallpapers') ? <span className="ob-spinner ob-row-spinner" /> : <span aria-hidden>+</span>}
            {w.add}
          </button>
        </div>
        <Row
          title={w.addFolder}
          link
          accessory={spinning('wallpaperFolder') ? <Checking /> : undefined}
          onClick={() => void add('wallpaperFolder', () => addWallpapers(true, w.pickWallpaperFolder, w.images))}
        />
      </Group>
      {report?.section === 'wallpapers' && <AddReport report={report.added} cw={cw} />}
      <p className="cz-note ob-in">{w.wallpapersNote}</p>
      <Group title={w.icons} place={5}>
        <IconSetRow
          name={w.westplay}
          icons={null}
          chosen={!look?.iconSet}
          shown={shown}
          cw={cw}
          choose={() => choose({ iconSet: null })}
        />
        {(custom?.iconSets ?? []).map((set) => (
          <IconSetRow
            key={set.name}
            name={set.name}
            icons={set}
            chosen={look?.iconSet === set.name}
            shown={shown}
            cw={cw}
            choose={() => choose({ iconSet: set.name })}
            remove={() => void remove(removeIconSet(set.name))}
          />
        ))}
        <Row
          title={w.addFolder}
          link
          accessory={spinning('folder') ? <Checking /> : undefined}
          onClick={() => void add('folder', () => addIconSet(false, w.pickFolder, w.zips))}
        />
        <Row
          title={w.addZip}
          link
          accessory={spinning('zip') ? <Checking /> : undefined}
          onClick={() => void add('zip', () => addIconSet(true, w.pickZip, w.zips))}
        />
      </Group>
      {report?.section === 'icons' && <AddReport report={report.added} cw={cw} />}
      <div className="cz-style ob-in" role="radiogroup" aria-label={w.style.title}>
        <span>{w.style.title}</span>
        <span className="cz-segments">
          {STYLES.map((style) => (
            <button
              key={style}
              type="button"
              role="radio"
              aria-checked={(look?.iconStyle ?? 'auto') === style}
              onClick={() => choose({ iconStyle: style })}
            >
              {w.style[style]}
            </button>
          ))}
        </span>
      </div>
      <p className="cz-note ob-in">
        {w.iconsNote.replace('{names}', names)} {w.looksNote}
      </p>
    </Panel>
  )
}

function WallpaperTile({
  wallpaper,
  chosen,
  pair,
  cw,
  choose,
  remove,
}: {
  wallpaper: Wallpaper
  chosen: boolean
  pair: boolean
  cw: CompanionWords
  choose(): void
  remove?: () => void
}) {
  const src = wallpaper.variants.dark ?? wallpaper.variants.light
  return (
    <div className="cz-wallpaper" data-chosen={chosen || undefined}>
      <button type="button" className="cz-wallpaper-choose" aria-pressed={chosen} onClick={choose}>
        <img src={src} alt="" loading="lazy" />
        <span className="cz-wallpaper-name">{wallpaper.name}</span>
        {pair && <span className="cz-wallpaper-pair">{cw.customize.pair}</span>}
      </button>
      {remove && (
        <button
          type="button"
          className="cz-remove"
          aria-label={`${cw.customize.remove} ${wallpaper.name}`}
          onClick={remove}
        >
          <RemoveGlyph />
        </button>
      )}
    </div>
  )
}

/** An icon set: its name, how many apps it has icons for, its icons in the look shown (WestPlay's where it
 * has none), and a tick when it's the one shown. */
function IconSetRow({
  name,
  icons,
  chosen,
  shown,
  cw,
  choose,
  remove,
}: {
  name: string
  /** `null`: WestPlay's own. */
  icons: IconSet | null
  chosen: boolean
  shown: 'light' | 'dark'
  cw: CompanionWords
  choose(): void
  remove?: () => void
}) {
  const own = icons ? Object.keys(icons.icons).length : ICON_FILES.length
  return (
    <div className="ob-row cz-set" data-chosen={chosen || undefined}>
      <button type="button" className="cz-set-choose" aria-pressed={chosen} onClick={choose}>
        <span className="cz-set-text">
          <span className="cz-set-head">
            <b title={name}>{name}</b>
            {icons && <span>{cw.customize.iconCount.replace('{n}', String(own))}</span>}
          </span>
          <span className="cz-strip" aria-hidden>
            {ICON_FILES.map(([role]) => {
              const path = icons?.icons[role]?.[shown]
              return (
                <img
                  key={role}
                  src={path ? customFile(path) : appIcon(role, shown)}
                  alt=""
                  data-own={!path || undefined}
                />
              )
            })}
          </span>
        </span>
        <span className="cz-set-tick">{chosen && <Tick done />}</span>
      </button>
      {remove && (
        <button type="button" className="cz-remove" aria-label={`${cw.customize.remove} ${name}`} onClick={remove}>
          <RemoveGlyph />
        </button>
      )}
    </div>
  )
}

/** What adding did: what was added, and each file left out, with why. */
function AddReport({ report, cw }: { report: Added; cw: CompanionWords }) {
  const w = cw.customize
  const words = useWords()
  return (
    <Note tone={report.added.length ? 'calm' : 'bad'}>
      <b>{report.added.length ? w.added.replace('{names}', list(words, report.added)) : w.nothingAdded}</b>
      {report.problems.map((problem) => (
        <span key={problem.file} className="cz-problem">
          {problem.file}: {w.problems[problem.reason]}
        </span>
      ))}
    </Note>
  )
}

function RemoveGlyph() {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round">
      <path d="M8 8l8 8M16 8l-8 8" />
    </svg>
  )
}
