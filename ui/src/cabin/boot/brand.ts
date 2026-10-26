/** The game's own truck brand ids (def/vehicle/truck in the base game and its DLCs). */
export const GAME_BRANDS = ['daf', 'iveco', 'man', 'mercedes', 'renault', 'scania', 'volvo'] as const

/**
 * Mod vehicles whose brand isn't one of the game's (or isn't named at all), by their brand id: the brand's logo to
 * show. Oyuncuyusbis' Travego 16 SHD reports `t16shd` and "Travego 16SHD (OB)": a Mercedes-Benz; their Setras
 * report "Setra" and "Daimler Setra S531DT" (owner 2026-10-24).
 */
const MOD_BRANDS: Readonly<Record<string, string>> = { t16shd: 'mercedes', setra516hd: 'setra', setradt531: 'setra' }

/**
 * The brand ids to look for a truck's logo under, in order: its own (`brandId`, the game's or a mod's), the
 * brand of a mod vehicle that doesn't name it (`MOD_BRANDS`), then the game's brand a mod's truck is a version
 * of, found as a word in its brand id or its brand's name (a modded DAF XG+ reports `new_daf` and "DAF"; a
 * Mercedes-Benz one, "Mercedes-Benz").
 */
export function logoBrands(brandId: string, brand: string): string[] {
  const words = `${brandId} ${brand}`.toLowerCase().split(/[^a-z0-9]+/)
  const found = GAME_BRANDS.filter((game) => words.includes(game))
  return [...new Set([brandId, MOD_BRANDS[brandId] ?? '', ...found])].filter((id) => id !== '')
}
