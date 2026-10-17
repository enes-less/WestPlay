/** The game's own truck brand ids (def/vehicle/truck in the base game and its DLCs). */
export const GAME_BRANDS = ['daf', 'iveco', 'man', 'mercedes', 'renault', 'scania', 'volvo'] as const

/**
 * The brand ids to look for a truck's logo under, in order: its own (`brandId`, the game's or a mod's),
 * then the game's brand a mod's truck is a version of, found as a word in its brand id or its brand's name
 * (a modded DAF XG+ reports `new_daf` and "DAF"; a Mercedes-Benz one, "Mercedes-Benz").
 */
export function logoBrands(brandId: string, brand: string): string[] {
  const words = `${brandId} ${brand}`.toLowerCase().split(/[^a-z0-9]+/)
  const found = GAME_BRANDS.filter((game) => words.includes(game))
  return [...new Set([brandId, ...found])].filter((id) => id !== '')
}
