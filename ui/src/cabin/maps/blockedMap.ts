/**
 * What the Maps app says instead of the map when the player plays on a map WestPlay doesn't allow (the host
 * tells: a map by HVT, Heavy Vehicles Turkey), in the game's language: every language the game has, by
 * its own codes (`locale/<code>` in its archives). HVT and WestPlay are names, the same in every language.
 * The game shows Silesian players Polish (pl_si), and so does this.
 */
export const HVT_MAP_MESSAGES: Record<string, string> = {
  bg_bg: 'Картите от HVT не са разрешени в WestPlay. Свържете се с WestPlay, за да научите повече.',
  ca_es: "Els mapes d'HVT no estan permesos a WestPlay. Contacta amb WestPlay per saber-ne més.",
  cs_cz: 'Mapy od HVT nejsou ve WestPlay povoleny. Pro více informací kontaktujte WestPlay.',
  da_dk: 'Kort fra HVT er ikke tilladt i WestPlay. Kontakt WestPlay for at få mere at vide.',
  de_de: 'Karten von HVT sind in WestPlay nicht erlaubt. Kontaktiere WestPlay, um mehr zu erfahren.',
  el_gr: 'Οι χάρτες της HVT δεν επιτρέπονται στο WestPlay. Επικοινωνήστε με το WestPlay για να μάθετε περισσότερα.',
  en_gb: 'Maps by HVT are not allowed in WestPlay. Contact WestPlay to learn more.',
  en_us: 'Maps by HVT are not allowed in WestPlay. Contact WestPlay to learn more.',
  es_es: 'Los mapas de HVT no están permitidos en WestPlay. Contacta con WestPlay para saber más.',
  es_la: 'Los mapas de HVT no están permitidos en WestPlay. Comunícate con WestPlay para saber más.',
  et_ee: 'HVT kaardid ei ole WestPlays lubatud. Lisateabe saamiseks võta ühendust WestPlayga.',
  eu_es: 'HVTren mapak ez daude baimenduta WestPlayn. Jarri harremanetan WestPlayrekin gehiago jakiteko.',
  fi_fi: 'HVT:n kartat eivät ole sallittuja WestPlayssa. Ota yhteyttä WestPlayhin saadaksesi lisätietoja.',
  fr_ca: 'Les cartes de HVT ne sont pas permises dans WestPlay. Communiquez avec WestPlay pour en savoir plus.',
  fr_fr: 'Les cartes de HVT ne sont pas autorisées dans WestPlay. Contactez WestPlay pour en savoir plus.',
  gl_es: 'Os mapas de HVT non están permitidos en WestPlay. Contacta con WestPlay para saber máis.',
  hr_hr: 'Karte od HVT-a nisu dopuštene u WestPlayu. Za više informacija obratite se WestPlayu.',
  hu_hu: 'A HVT térképei nem engedélyezettek a WestPlayben. További információért lépj kapcsolatba a WestPlayjel.',
  it_it: 'Le mappe di HVT non sono consentite in WestPlay. Contatta WestPlay per saperne di più.',
  ja_jp: 'HVTのマップはWestPlayでは使用できません。詳しくはWestPlayにお問い合わせください。',
  ka_ge: 'HVT-ის რუკები WestPlay-ში დაშვებული არ არის. მეტის გასაგებად დაუკავშირდით WestPlay-ს.',
  ko_kr: 'HVT의 지도는 WestPlay에서 허용되지 않습니다. 자세한 내용은 WestPlay에 문의하세요.',
  lt_lt: 'HVT žemėlapiai WestPlay neleidžiami. Norėdami sužinoti daugiau, susisiekite su WestPlay.',
  lv_lv: 'HVT kartes WestPlay nav atļautas. Lai uzzinātu vairāk, sazinieties ar WestPlay.',
  mk_mk: 'Мапите од HVT не се дозволени во WestPlay. Контактирајте го WestPlay за да дознаете повеќе.',
  nl_nl: 'Kaarten van HVT zijn niet toegestaan in WestPlay. Neem contact op met WestPlay voor meer informatie.',
  no_no: 'Kart fra HVT er ikke tillatt i WestPlay. Kontakt WestPlay for å finne ut mer.',
  pl_pl: 'Mapy od HVT nie są dozwolone w WestPlay. Skontaktuj się z WestPlay, aby dowiedzieć się więcej.',
  pl_si: 'Mapy od HVT nie są dozwolone w WestPlay. Skontaktuj się z WestPlay, aby dowiedzieć się więcej.',
  pt_br: 'Mapas da HVT não são permitidos no WestPlay. Entre em contato com o WestPlay para saber mais.',
  pt_pt: 'Os mapas da HVT não são permitidos no WestPlay. Contacte o WestPlay para saber mais.',
  ro_ro: 'Hărțile HVT nu sunt permise în WestPlay. Contactează WestPlay pentru a afla mai multe.',
  ru_ru: 'Карты от HVT не разрешены в WestPlay. Свяжитесь с WestPlay, чтобы узнать больше.',
  sk_sk: 'Mapy od HVT nie sú vo WestPlay povolené. Pre viac informácií kontaktujte WestPlay.',
  sl_sl: 'Zemljevidi HVT v WestPlayu niso dovoljeni. Za več informacij se obrnite na WestPlay.',
  sr_sp: 'Mape od HVT-a nisu dozvoljene u WestPlay-u. Kontaktirajte WestPlay da saznate više.',
  sr_sr: 'Мапе од HVT-а нису дозвољене у WestPlay-у. Контактирајте WestPlay да сазнате више.',
  sv_se: 'Kartor från HVT är inte tillåtna i WestPlay. Kontakta WestPlay för att få veta mer.',
  tr_tr: "HVT haritalarına WestPlay'de izin verilmiyor. Daha fazla bilgi için WestPlay ile iletişime geçin.",
  uk_uk: "Карти від HVT не дозволені у WestPlay. Зв'яжіться з WestPlay, щоб дізнатися більше.",
  vi_vn: 'Bản đồ của HVT không được phép sử dụng trong WestPlay. Liên hệ WestPlay để tìm hiểu thêm.',
  zh_cn: 'WestPlay 不允许使用 HVT 的地图。如需了解详情，请联系 WestPlay。',
  zh_tw: 'WestPlay 不允許使用 HVT 的地圖。如需瞭解詳情，請聯絡 WestPlay。',
}

/** The message in the game's `language`, or English for one not listed (a mod's). */
export function hvtMapMessage(language: string | null): string {
  return HVT_MAP_MESSAGES[language ?? ''] ?? HVT_MAP_MESSAGES.en_us
}
