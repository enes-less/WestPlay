# The tablet

The model WestPlay's tablet accessory is made from: a tablet mod by **Kedi739**
([YouTube](https://www.youtube.com/channel/UCyYmKt1vNY1Wg47hjOKnnag), Discord `youtube_kedi739`), who gave
WestPlay full permission to use it and change it as it likes (2026-10-24).

These are his files as he made them:

- `Kedi739.pmd`, `Kedi739.pmg`: the model, a tablet with a small second device beside it;
- `tablet.dds`: the tablet's texture.

The mod builder (`crates/setup/src/tablet.rs`) makes WestPlay's tablet from them on the player's PC. It
leaves out the small device, puts the model's origin at the tablet's base, and shows WestPlay's screen
texture on the tablet's screen.
