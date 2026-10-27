# The tablet

WestPlay's tablet accessory is an iPad Air 5: **"Ipad Air 5 (FREE)" by Art_bor**
([Sketchfab](https://sketchfab.com/3d-models/ipad-air-5-free-628ab0359d774af480be8eda9de70272)), licensed under
[Creative Commons Attribution 4.0](https://creativecommons.org/licenses/by/4.0/) (credited in
[THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md)).

- `ipad.pmg`: the model in the game's format, made from the model's `.glb` (Sketchfab's "GLB" download) by
  `python tools/dev/tablet_model.py <ipad_air_5_free.glb>`: landscape, at its true size (247.6 x 178.5 x 6.1 mm),
  two pieces: the body and the glass. Left out: Sketchfab's ground plane and the glass's picture (Apple's
  wallpaper).
- `ipad.dds`: the body's colour texture, a patch of each look's colour (aluminium, buttons and camera ring,
  camera glass, connector, the black glass round the screen), written by the same tool.
- `Kedi739.pmd`, `Kedi739.pmg`: the first tablet, a tablet mod by **Kedi739**
  ([YouTube](https://www.youtube.com/channel/UCyYmKt1vNY1Wg47hjOKnnag), Discord `youtube_kedi739`), who gave
  WestPlay full permission to use it and change it as it likes (2026-10-24). No longer shown: kept as the
  pattern the tablet's files are laid out as (its descriptor, by `tablet.rs`; its model's header and pieces'
  records, by the tool).

The mod builder (`crates/setup/src/tablet.rs`) makes WestPlay's tablet from them on the player's PC: it puts the
model's origin at the tablet's base (on its stand) or its top (hanging), and shows WestPlay's screen texture on
the glass.
