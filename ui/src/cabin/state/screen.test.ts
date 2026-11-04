import { describe, expect, it } from 'vitest'
import layoutsFile from '../../../../crates/shared/screen_layouts.txt?raw'
import type { TruckInfo } from '../../shared/host'
import {
  FALLBACK_PROFILE,
  findProfile,
  returnButtonCentre,
  isScreenLayoutId,
  pageHeight,
  pageRows,
  type ProfileEntry,
  SCREEN_LAYOUT_IDS,
  screenProfile,
} from './screen'
import sharedLib from '../../../../crates/shared/src/lib.rs?raw'

function truck(brandId: string, id: string): TruckInfo {
  return { brandId, brand: brandId.toUpperCase(), id, name: '' }
}

const scaniaS = truck('scania', 'vehicle.scania.s_2016')
const dafXg = truck('daf', 'vehicle.daf.2021')

describe('screenProfile', () => {
  it('picks the profile by the layout the game draws, not by the truck alone', () => {
    // The same Scania S has a different screen with the Smart Dash interior than with the classic one.
    expect(screenProfile('scania-2025', scaniaS).oemStrip).toBe('scania-2025')
    // Its Smart Dash Advanced: the same strip, everything smaller on the larger screen.
    expect(screenProfile('scania-2025-advanced', scaniaS)).toMatchObject({
      oemStrip: 'scania-2025',
      scale: 0.78,
      recents: 4,
    })
    expect(screenProfile('scania-2025', scaniaS).scale).toBe(0.9)
    // Its 7" screen: tuned larger (a small screen), while another truck on the stock screen isn't tuned.
    expect(screenProfile('generic', scaniaS).scale).toBe(1.7)
    expect(screenProfile('generic', truck('mercedes', 'vehicle.mercedes.actros'))).toBe(FALLBACK_PROFILE)
    expect(screenProfile('daf-2021', dafXg).oemStrip).toBe('daf-2021')
  })

  it('accepts model ids with or without the vehicle prefix', () => {
    expect(screenProfile('daf-2021', truck('daf', 'daf.2021'))).toBe(screenProfile('daf-2021', dafXg))
  })

  it('falls back for screens not tuned yet, and before any screen was drawn', () => {
    expect(screenProfile('renault-t-2024', truck('renault', 'vehicle.renault.t_2024'))).toBe(FALLBACK_PROFILE)
    expect(screenProfile(null, dafXg)).toBe(FALLBACK_PROFILE)
    expect(screenProfile('daf-2021', null).oemStrip).toBe('daf-2021')
  })
})

describe('findProfile', () => {
  const classic = { name: 'Classic', oemStrip: null, scale: 1 }
  const classicScaniaS = { name: 'Scania S classic', oemStrip: null, scale: 1.1 }
  const entries: ProfileEntry[] = [
    { layout: 'generic', models: ['scania.s_2016'], profile: classicScaniaS },
    { layout: 'generic', profile: classic },
  ]

  it('prefers an entry for the truck model over the one for its layout alone', () => {
    expect(findProfile(entries, 'generic', scaniaS)).toBe(classicScaniaS)
    expect(findProfile(entries, 'generic', dafXg)).toBe(classic)
    expect(findProfile(entries, 'generic', null)).toBe(classic)
  })

  it('never applies a model entry to another layout', () => {
    expect(findProfile(entries, 'scania-2025', scaniaS)).toBe(FALLBACK_PROFILE)
  })
})

describe('isScreenLayoutId', () => {
  it('accepts the layouts the plugin reports and nothing else', () => {
    expect(isScreenLayoutId('scania-2025')).toBe(true)
    expect(isScreenLayoutId('volvo-2030')).toBe(false)
    expect(isScreenLayoutId(null)).toBe(false)
  })
})

describe('SCREEN_LAYOUT_IDS', () => {
  it('lists the layouts in the order of crates/shared/screen_layouts.txt', () => {
    const ids = layoutsFile
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line !== '' && !line.startsWith('#'))
      .map((line) => line.split(/\s+/)[0])
    expect(SCREEN_LAYOUT_IDS).toEqual(ids)
  })
})

describe('pageHeight', () => {
  it("is crates/shared's page_height for every layout", () => {
    // The Rust function's arms, `Some("id") => height` (several ids to an arm: `"a" | "b"`).
    const body = sharedLib.slice(sharedLib.indexOf('pub fn page_height'), sharedLib.indexOf('_ => SCREEN_HEIGHT'))
    const rust = new Map<string, number>()
    for (const [, ids, height] of body.matchAll(/Some\(([^)]*)\) => (\d+)/g)) {
      for (const [, id] of ids.matchAll(/"([^"]+)"/g)) rust.set(id, Number(height))
    }
    expect(rust.size).toBeGreaterThan(5)
    for (const layout of SCREEN_LAYOUT_IDS) expect(pageHeight(layout), layout).toBe(rust.get(layout) ?? 720)
    expect(pageHeight(null)).toBe(720)
  })

  it('turns frame rows into the page rows that show where they do', () => {
    // The Smart Dash's display, 1.633:1: the page 784 tall, its rows 720/784 of a frame row each.
    expect(pageRows(720, 'scania-2025')).toBe(784)
    expect(pageRows(90, 'scania-2025')).toBe(98)
    expect(pageRows(106, 'daf-2021')).toBe(106)
  })
})

describe('returnButtonCentre', () => {
  it("sits in the CarPlay area's top-left corner, or where the screen's profile puts it", () => {
    expect(returnButtonCentre(FALLBACK_PROFILE, 0, null)).toEqual({ x: 68, y: 68 })
    expect(returnButtonCentre(FALLBACK_PROFILE, 94, 'daf-2021')).toEqual({ x: 162, y: 68 })
    // The Smart Dash's spot (frame pixels, between the stock map's buttons) where it shows on its taller page.
    const smartDash = returnButtonCentre(screenProfile('scania-2025', scaniaS), 94, 'scania-2025')
    expect(smartDash.x).toBe(148)
    expect(smartDash.y).toBeCloseTo((308 * 784) / 720)
    // Only the Scania's screen has a rim the image's edge colours: its dash's own dark grey.
    expect(screenProfile('scania-2025', scaniaS).rim).toBe('#0d0f13')
    expect(screenProfile('daf-2021', null).rim).toBeUndefined()
  })
})
