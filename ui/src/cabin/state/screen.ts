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
  'scania-touring',
  'man-lions-24',
] as const

export type ScreenLayoutId = (typeof SCREEN_LAYOUT_IDS)[number]

export function isScreenLayoutId(value: unknown): value is ScreenLayoutId {
  return SCREEN_LAYOUT_IDS.includes(value as ScreenLayoutId)
}

/** The app's frame size: the canvas every screen layout is designed on. */
export const SCREEN_WIDTH = 1280
export const SCREEN_HEIGHT = 720

/**
 * The height the page is laid out at on this layout's screen, its width SCREEN_WIDTH: the shape of the part of
 * the screen its display shows, so the UI fits it rather than being stretched to it (crates/shared `page_height`,
 * which the host lays the page out by; a test keeps the two the same). The page is squeezed into the frame's
 * SCREEN_HEIGHT rows, and the game stretches it back.
 */
export function pageHeight(layout: ScreenLayoutId | null): number {
  return (layout && PAGE_HEIGHTS[layout]) ?? SCREEN_HEIGHT
}

const PAGE_HEIGHTS: Partial<Record<ScreenLayoutId, number>> = {
  generic: 793,
  'scania-2025': 784,
  'scania-2025-advanced': 711,
  'iveco-sway': 717,
  'volvo-fh': 731,
  tablet: 898,
  'travego-16': 757,
  'travego-16-23': 757,
  'setra-516': 777,
  'setra-531': 761,
  'man-lions-coach': 778,
  'scania-touring': 730,
  'man-lions-24': 784,
}

/** Frame pixels (SCREEN_WIDTH x SCREEN_HEIGHT, as a screen's script and the truck's strip are measured) as the
 * page's on this layout's screen, down the screen: across, they're the same. */
export function pageRows(rows: number, layout: ScreenLayoutId | null): number {
  return (rows * pageHeight(layout)) / SCREEN_HEIGHT
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
  /** How much larger the sidebar's app icons are than every other screen's (1 when not set). */
  sidebarIcons?: number
  /** How large the sidebar's buttons (Home and Dashboard, the truck) are, their symbols with them, against their first
   * size ([`SIDEBAR_BUTTONS`] when not set): each stays as far from the sidebar's bottom at any size. */
  sidebarButtons?: number
  /** How many of the sidebar's app slots fit (three when not set; `sidebarApps`, four at most): a tall screen's
   * has room for the fourth, a short one's for fewer. */
  recents?: number
  /** A short sidebar's spacing (page units), tighter than every other screen's so its apps stay in its middle clear of
   * the clock and the buttons: between the apps (shell.css: 18), the Home symbol from the bottom (58.67), and between
   * the truck and Home buttons' symbols (36). */
  sidebarGaps?: { apps?: number; homeBottom?: number; buttons?: number }
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
   * The margin round CarPlay, its sidebar and screens (frame pixels; shell.css's `--edge`, 16 when not set: every
   * screen's, owner 2026-11-03).
   */
  edge?: number
}

/** The WestPlay button's size, and its margin from the CarPlay area's corner (shell.css --edge). */
export const RETURN_BUTTON_SIZE = 72
const RETURN_BUTTON_MARGIN = 32

/** The WestPlay button's centre while the screen is handed to the truck, in the page's pixels on `layout`'s
 * screen. `areaLeft`: where CarPlay starts. */
export function returnButtonCentre(
  profile: ScreenProfile,
  areaLeft: number,
  layout: ScreenLayoutId | null,
): { x: number; y: number } {
  const half = RETURN_BUTTON_SIZE / 2
  const spot = profile.returnButton
  return spot
    ? { x: spot.x, y: pageRows(spot.y, layout) }
    : { x: areaLeft + RETURN_BUTTON_MARGIN + half, y: RETURN_BUTTON_MARGIN + half }
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
  // Its sidebar ends above the DAF's strip, short for three apps in its middle (owner 2026-11-13) and both buttons: the
  // apps, the buttons and the Home button from the bottom closer, chosen by eye from several, so the apps stay in the
  // middle, the truck nearer the Home button than them, and the Home button clear of the round end.
  {
    layout: 'daf-2021',
    profile: {
      name: 'DAF 2021',
      oemStrip: 'daf-2021',
      scale: 1,
      sidebarGaps: { apps: 10, homeBottom: 40, buttons: 18 },
    },
  },
  {
    layout: 'generic',
    models: ['scania.s_2016', 'scania.r_2016'],
    profile: {
      name: 'Scania 7"',
      oemStrip: null,
      // A small screen (about 15 cm across) shown at 254 x 157 pixels of the game's: everything about 1.7
      // times larger, for the same real size of text as on the DAF's screen (the Home grid has fewer columns).
      scale: 1.7,
      // Three sidebar apps, as every screen's (owner 2026-11-13: "clearly enough space"; two since 2026-10-21, when
      // its sidebar's buttons and spacing took more).
      // Its margin and the gaps between its panels smaller than every other screen's 16, for the room (owner
      // 2026-11-03).
      edge: 10,
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
      // Its sidebar is taller in the UI's own units: room for all four.
      recents: 4,
    },
  },
  // Volvo FH 2021 and 2024: the whole screen, at the DAF's sizing until tuned in the cab (owner 2026-10-21).
  { layout: 'volvo-fh', profile: { name: 'Volvo FH', oemStrip: null, scale: 1 } },
  // WestPlay's tablet, in any truck: an iPad Air's glass, 23.2 x 16.3 cm, the page laid out at its 1.425:1
  // (crates/shared page_height), at the DAF's sizing. Its glass shows the frame whole. Its sidebar's icons a quarter
  // larger and its Home button twice the size, chosen by the owner from drawings of it (2026-11-11: smaller than a
  // finger could touch).
  {
    layout: 'tablet',
    profile: { name: 'WestPlay tablet', oemStrip: null, scale: 1, sidebarIcons: 1.25, sidebarButtons: 2 },
  },
  // Oyuncuyusbis' coaches (mods, owner 2026-10-24), each on its own screen, about 15-16 x 9-10 cm (the Setra S 516
  // HD/2 on its tablet part, 20.1 x 12.2 cm: owner 2026-10-26), the page laid out at its display's shape (crates/shared
  // page_height): at the DAF's sizing until tuned in the cab.
  { layout: 'travego-16', profile: { name: 'Mercedes-Benz Travego 16 SHD', oemStrip: null, scale: 1 } },
  { layout: 'travego-16-23', profile: { name: 'Mercedes-Benz Travego 16 SHD (2023)', oemStrip: null, scale: 1 } },
  { layout: 'setra-516', profile: { name: 'Setra S 516 HD/2', oemStrip: null, scale: 1 } },
  { layout: 'setra-531', profile: { name: 'Setra S 531 DT', oemStrip: null, scale: 1 } },
  { layout: 'man-lions-coach', profile: { name: "MAN Lion's Coach", oemStrip: null, scale: 1 } },
  // The Coaches DLC's (owner 2026-11-12: their real ones have CarPlay), each on its own screen, the page laid out at
  // its display's shape (crates/shared page_height). The Scania Touring's 19.4 x 11.1 cm at the DAF's sizing, as the
  // Volvo FH's of its size.
  { layout: 'scania-touring', profile: { name: 'Scania Touring', oemStrip: null, scale: 1 } },
  // The MAN Lion's Coach's 15.7 x 9.6 cm: at the DAF's 1 everything too small, at its real size (1.43) far too large
  // (owner 2026-11-12, 13). At 1.2, chosen by eye in the cab preview: three columns of apps. Two sidebar apps (owner
  // 2026-11-13, after seeing three).
  { layout: 'man-lions-24', profile: { name: "MAN Lion's Coach (2024)", oemStrip: null, scale: 1.2, recents: 2 } },
]

/** The sidebar's buttons on every screen: the tablet's twice their first size (owner 2026-11-11: smaller than a finger
 * could touch), then every screen's (owner 2026-11-12: "too small globally"). At twice, beside a sidebar app of the
 * first size their symbols were as large as it ("fucking huge", owner 2026-11-13): 1.6, as the tablet's to its icons a
 * quarter larger, and about CarPlay's (reference/carplay-photos: its Home symbol half its icons). */
export const SIDEBAR_BUTTONS = 1.6

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
