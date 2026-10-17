import { describe, expect, it } from 'vitest'
import { HVT_MAP_MESSAGES, hvtMapMessage } from './blockedMap'

/** Every language the game has (`locale/` in its archives). */
const GAME_LANGUAGES =
  'bg_bg ca_es cs_cz da_dk de_de el_gr en_gb en_us es_es es_la et_ee eu_es fi_fi fr_ca fr_fr gl_es hr_hr hu_hu it_it ja_jp ka_ge ko_kr lt_lt lv_lv mk_mk nl_nl no_no pl_pl pl_si pt_br pt_pt ro_ro ru_ru sk_sk sl_sl sr_sp sr_sr sv_se tr_tr uk_uk vi_vn zh_cn zh_tw'.split(
    ' ',
  )

describe('the HVT map message', () => {
  it('is in every language the game has, naming HVT and WestPlay', () => {
    expect(Object.keys(HVT_MAP_MESSAGES).sort()).toEqual(GAME_LANGUAGES)
    for (const message of Object.values(HVT_MAP_MESSAGES)) {
      expect(message).toContain('HVT')
      expect(message).toContain('WestPlay')
    }
  })

  it('says it in the game language, or English', () => {
    expect(hvtMapMessage('en_us')).toBe('Maps by HVT are not allowed in WestPlay. Contact WestPlay to learn more.')
    expect(hvtMapMessage('tr_tr')).toMatch(/^HVT haritalarına/)
    expect(hvtMapMessage('xx_mod')).toBe(HVT_MAP_MESSAGES.en_us)
    expect(hvtMapMessage(null)).toBe(HVT_MAP_MESSAGES.en_us)
  })
})
