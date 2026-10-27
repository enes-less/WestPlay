import type { TruckInfo } from '../../shared/host'
import type { OemStripId } from './oem'

/**
 * The kinds of in-cabin screen WestPlay is drawn on: the game's GPS layouts the mod patches, in the order of
 * crates/shared/screen_layouts.txt (the order sets each layout's screen texture size; a test checks it). The
 * plugin reports which one the game is drawing.
 */
export const SCREEN_LAYOUT_IDS = [
  'generic',
  'daf-2021',
  'scania-2025',
  'renault-t-2024',
  'iveco-sway',
  'scania-2025-advanced',
  'volvo-fh',
  'tablet',
  'travego-16',
  'travego-16-23',
  'setra-516',
  'setra-531',
  'man-lions-coach',
] as const

export type ScreenLayoutId = (typeof SCREEN_LAYOUT_IDS)[number]

export function isScreenLayoutId(value: unknown): value is ScreenLayoutId {
  return SCREEN_LAYOUT_IDS.includes(value as ScreenLayoutId)
}

/** The app's frame size: the canvas every screen layout is designed on. */
export const SCREEN_WIDTH = 1280
export const SCREEN_HEIGHT = 720

/**
 * The frame row that appears at canvas row `y` on this layout's screen.
 *
 * Each layout's screen texture is SCREEN_HEIGHT plus the layout's index rows tall (that is how the plugin
 * tells layouts apart), and the game stretches it over the screen's SCREEN_HEIGHT units, so frame rows land
 * slightly higher than their number: the DAF 2021's 721 rows put row 614 at 613.15. Rounded up, so an
 * opaque area ending at the returned row always reaches `y`.
 */
export function frameRowAt(y: number, layout: ScreenLayoutId | null): number {
  const rows = SCREEN_HEIGHT + (layout ? SCREEN_LAYOUT_IDS.indexOf(layout) : 0)
  return Math.ceil((y * rows) / SCREEN_HEIGHT)
}

/** How WestPlay fits one truck's screen. */
export interface ScreenProfile {
  /** For development notices, e.g. "Scania Smart Dash". */
  name: string
  /** The truck's own strip that stays on screen around CarPlay (Glass style), if any. */
  oemStrip: OemStripId | null
  /**
   * Size of the CarPlay UI. Physically larger screens use less than 1, so icons and text keep a similar
   * real-world size and more fits on screen, like CarPlay on a bigger display (the Home grid gains columns).
   */
  scale: number
  /** How many recent apps the sidebar shows (three when not set): a tall screen's has room for more, a short
   * one's for fewer. */
  recents?: number
  /**
   * Where WestPlay's button sits while the screen is handed to the truck (its centre, in frame pixels), if
   * the default spot (the CarPlay area's top-left corner) covers part of the truck's own screen.
   */
  returnButton?: { x: number; y: number }
  /**
   * The screen is larger than the image on it, and the game fills the rest, a rim round it, by repeating the
   * image's outermost pixels outwards: keep them this colour (a CSS colour; a cursor in a corner would
   * otherwise light the rim).
   */
  rim?: string
  /**
   * The margin round CarPlay, its sidebar and screens (frame pixels; shell.css's `--edge`, 32 when not set: the
   * DAF's screen crops the image's edge a little). A screen showing the whole frame takes less.
   */
  edge?: number
}

/** The WestPlay button's size, and its margin from the CarPlay area's corner (shell.css --edge). */
export const RETURN_BUTTON_SIZE = 72
const RETURN_BUTTON_MARGIN = 32

/** The WestPlay button's centre while the screen is handed to the truck. `areaLeft`: where CarPlay starts. */
export function returnButtonCentre(profile: ScreenProfile, areaLeft: number): { x: number; y: number } {
  const half = RETURN_BUTTON_SIZE / 2
  return profile.returnButton ?? { x: areaLeft + RETURN_BUTTON_MARGIN + half, y: RETURN_BUTTON_MARGIN + half }
}

export interface ProfileEntry {
  layout: ScreenLayoutId
  /** Only these truck models (SDK model ids without the `vehicle.` prefix, e.g. `scania.s_2016`). */
  models?: readonly string[]
  profile: ScreenProfile
}

/** The Scania Smart Dash (10.1", 23.4 x 14.3 cm): tuned in the cab. */
const SMART_DASH: ScreenProfile = {
  name: 'Scania Smart Dash',
  oemStrip: 'scania-2025',
  // A larger display than the DAF's: everything a little smaller, so the Home grid gets a fifth column.
  scale: 0.9,
  // The stock map has round buttons in a column right of the strip (menu and search at the top, recentre
  // below; ui/dashboard/scania_2025_gps.sii, x 137-197 of 1440): WestPlay's goes in the free stretch
  // between them, like one of the dash's own.
  returnButton: { x: 148, y: 308 },
  // Its rim (the screen beyond the image) in the colour of the dash's own strip below the screen
  // (scania_2025_gps.sii: FF130F0D, alpha-blue-green-red, on white).
  rim: '#0d0f13',
}

/**
 * Screens tuned in the cab, truck by truck. An entry with `models` applies to those trucks only and wins
 * over the entry for its layout alone (one layout can sit in several trucks with different screen sizes).
 */
const PROFILES: readonly ProfileEntry[] = [
  { layout: 'daf-2021', profile: { name: 'DAF 2021', oemStrip: 'daf-2021', scale: 1 } },
  {
    layout: 'generic',
    models: ['scania.s_2016', 'scania.r_2016'],
    profile: {
      name: 'Scania 7"',
      oemStrip: null,
      // A small screen (about 15 cm across) shown at 254 x 157 pixels of the game's: everything about 1.7
      // times larger, for the same real size of text as on the DAF's screen (the Home grid has fewer columns).
      scale: 1.7,
      // Its short sidebar: two recent apps (owner 2026-10-21).
      recents: 2,
    },
  },
  { layout: 'scania-2025', profile: SMART_DASH },
  {
    layout: 'scania-2025-advanced',
    profile: {
      ...SMART_DASH,
      name: 'Scania Smart Dash Advanced',
      // The same screen's script on a larger display (12.9", 31.5 x 17.5 cm: 1.35 times as wide, 1.22 as
      // tall). The 10.1"'s 0.9 by the screens' diagonals (0.78) would be 0.7, the same real size; at that,
      // Maps' small print was too small to read (owner 2026-10-21), so a little larger than the 10.1".
      scale: 0.78,
      // Its sidebar is taller in the UI's own units: room for five recent apps.
      recents: 5,
    },
  },
  // Volvo FH 2021 and 2024: the whole screen, at the DAF's sizing until tuned in the cab (owner 2026-10-21).
  { layout: 'volvo-fh', profile: { name: 'Volvo FH', oemStrip: null, scale: 1 } },
  // WestPlay's tablet, in any truck: an iPad Air's glass, 23.2 x 16.3 cm, the page laid out at its 1.425:1
  // (crates/shared page_height), at the DAF's sizing until tuned in the cab. Its glass shows the frame whole: the
  // 7"'s small margin (owner 2026-10-27: "too much margin").
  { layout: 'tablet', profile: { name: 'WestPlay tablet', oemStrip: null, scale: 1, edge: 14 } },
  // Oyuncuyusbis' coaches (mods, owner 2026-10-24), each on its own screen, about 15-16 x 9-10 cm (the Setra S 516
  // HD/2 on its tablet part, 20.1 x 12.2 cm: owner 2026-10-26), the page laid out at its display's shape (crates/shared
  // page_height): at the DAF's sizing until tuned in the cab.
  { layout: 'travego-16', profile: { name: 'Mercedes-Benz Travego 16 SHD', oemStrip: null, scale: 1 } },
  { layout: 'travego-16-23', profile: { name: 'Mercedes-Benz Travego 16 SHD (2023)', oemStrip: null, scale: 1 } },
  { layout: 'setra-516', profile: { name: 'Setra S 516 HD/2', oemStrip: null, scale: 1 } },
  { layout: 'setra-531', profile: { name: 'Setra S 531 DT', oemStrip: null, scale: 1 } },
  { layout: 'man-lions-coach', profile: { name: "MAN Lion's Coach", oemStrip: null, scale: 1 } },
]

/** Screens not tuned yet: CarPlay on the whole screen, its layout adapting to the space (responsive). */
export const FALLBACK_PROFILE: ScreenProfile = { name: 'Default', oemStrip: null, scale: 1 }

/** The profile for the screen the game draws WestPlay on (`layout` is null until it has drawn one). */
export function screenProfile(layout: ScreenLayoutId | null, truck: TruckInfo | null): ScreenProfile {
  return findProfile(PROFILES, layout, truck)
}

export function findProfile(
  entries: readonly ProfileEntry[],
  layout: ScreenLayoutId | null,
  truck: TruckInfo | null,
): ScreenProfile {
  if (layout === null) return FALLBACK_PROFILE
  const model = truck ? modelId(truck) : null
  const candidates = entries.filter((entry) => entry.layout === layout)
  const forModel = candidates.find((entry) => model !== null && entry.models?.includes(model))
  const forLayout = candidates.find((entry) => entry.models === undefined)
  return (forModel ?? forLayout)?.profile ?? FALLBACK_PROFILE
}

/** `vehicle.scania.s_2016` -> `scania.s_2016` (the SDK reports the id with or without the prefix). */
function modelId(truck: TruckInfo): string {
  return truck.id.replace(/^vehicle\./, '')
}
