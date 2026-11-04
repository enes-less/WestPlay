/**
 * The game's languages by their own names, as the game's language menu and iOS's list name them: each in
 * itself (Deutsch, Türkçe, 日本語), a language the game has twice told apart by where or how it's written.
 */
const OWN_NAMES: Record<string, string> = {
  bg_bg: 'Български',
  ca_es: 'Català',
  cs_cz: 'Čeština',
  da_dk: 'Dansk',
  de_de: 'Deutsch',
  el_gr: 'Ελληνικά',
  en_gb: 'English (UK)',
  en_us: 'English (US)',
  es_es: 'Español (España)',
  es_la: 'Español (Latinoamérica)',
  et_ee: 'Eesti',
  eu_es: 'Euskara',
  fi_fi: 'Suomi',
  fr_ca: 'Français (Canada)',
  fr_fr: 'Français (France)',
  gl_es: 'Galego',
  hr_hr: 'Hrvatski',
  hu_hu: 'Magyar',
  it_it: 'Italiano',
  ja_jp: '日本語',
  ka_ge: 'ქართული',
  ko_kr: '한국어',
  lt_lt: 'Lietuvių',
  lv_lv: 'Latviešu',
  mk_mk: 'Македонски',
  nl_nl: 'Nederlands',
  no_no: 'Norsk',
  pl_pl: 'Polski',
  pl_si: 'Ślōnsko godka',
  pt_br: 'Português (Brasil)',
  pt_pt: 'Português (Portugal)',
  ro_ro: 'Română',
  ru_ru: 'Русский',
  sk_sk: 'Slovenčina',
  sl_sl: 'Slovenščina',
  sr_sp: 'Srpski (latinica)',
  sr_sr: 'Српски (ћирилица)',
  sv_se: 'Svenska',
  tr_tr: 'Türkçe',
  uk_uk: 'Українська',
  vi_vn: 'Tiếng Việt',
  zh_cn: '简体中文',
  zh_tw: '繁體中文',
}

/** BCP 47 tags for the game's codes, to name a language in another (`Intl.DisplayNames`): its letters alone,
 * a region or script only for a language the game has twice (as its own names tell them apart). */
const TAGS: Record<string, string> = {
  en_gb: 'en-GB',
  en_us: 'en-US',
  es_es: 'es-ES',
  es_la: 'es-419',
  fr_ca: 'fr-CA',
  fr_fr: 'fr-FR',
  no_no: 'nb',
  pl_si: 'szl',
  pt_br: 'pt-BR',
  pt_pt: 'pt-PT',
  sr_sp: 'sr-Latn',
  sr_sr: 'sr-Cyrl',
  zh_cn: 'zh-Hans',
  zh_tw: 'zh-Hant',
}

/** The game's languages, by their own names in alphabetical order (Latin letters first). */
export function gameLanguages(): string[] {
  return Object.keys(OWN_NAMES).sort((a, b) => OWN_NAMES[a].localeCompare(OWN_NAMES[b], 'en'))
}

/** A language's own name (`de_de`: Deutsch); the code itself for one the game doesn't have. */
export function ownName(code: string): string {
  return OWN_NAMES[code] ?? code
}

/** A language's name in the language of `locale` (`de_de` in `tr-TR`: Almanca); `null` where that has no
 * name for it, or it's the language itself. */
export function nameIn(code: string, locale: string): string | null {
  const tag = TAGS[code] ?? code.slice(0, 2)
  try {
    const name = new Intl.DisplayNames([locale], { type: 'language', languageDisplay: 'dialect' }).of(tag)
    if (!name || name === tag || name === OWN_NAMES[code]) return null
    return name.charAt(0).toLocaleUpperCase(locale) + name.slice(1)
  } catch {
    return null
  }
}
