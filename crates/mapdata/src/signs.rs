//! The map's road number signs. The game marks its roads' numbers with map overlays (kind 0) naming a
//! sign (`d_a1`, `fr_e15`, `es_ap7`…), each drawn as the game's own image of that country's real sign
//! (`/material/ui/map/road/road_<sign>.tobj`): its shape, colours and lettering (A1(M), AP-7, D.100), none
//! of which the sign's name alone says. The signs used are gathered into one sprite sheet for the page
//! (MapLibre's format: a PNG and a JSON index of where each image is), read from the player's game like
//! the rest of the map and never shipped.

use std::collections::BTreeSet;

use serde_json::{Map as JsonMap, Value, json};

use crate::files::GameFiles;
use crate::sector::ItemData;
use crate::texture::Texture;
use crate::token::token_to_string;
use crate::world::Map;

/// Overlays of the same kind that aren't road numbers: toll gates, border crossings, weigh stations and
/// quarries (their images are masks, not pictures).
const NOT_SIGNS: [&str; 4] = ["toll_ico", "border_ico", "weigh_ico", "quarry"];

/// The sheet's width, pixels; images go in rows across it, a pixel apart (so none bleeds into another
/// when drawn scaled).
const SHEET_WIDTH: u32 = 1024;
const GAP: u32 = 1;
/// The images are drawn at half their size on screen (in CSS pixels): a 64-pixel sign as 32, sharp on the
/// cabin's screens.
const PIXEL_RATIO: u32 = 2;

/// The road sign an overlay shows, if it's one: its name.
pub fn road_sign(item: &ItemData) -> Option<String> {
    let ItemData::MapOverlay { flags, kind: 0, icon, .. } = item else { return None };
    if flags.hidden || flags.secret {
        return None;
    }
    let name = token_to_string(*icon)?;
    (!name.is_empty() && !NOT_SIGNS.contains(&name.as_str())).then_some(name)
}

/// Every road sign the map shows.
pub fn road_signs(map: &Map) -> BTreeSet<String> {
    map.items.iter().filter_map(|item| road_sign(&item.data)).collect()
}

/// A sprite sheet: the image, its index (MapLibre's sprite JSON), and the signs on it (those whose image
/// could be read).
pub struct Sprite {
    pub png: Vec<u8>,
    pub index: Value,
    pub signs: BTreeSet<String>,
}

/// The sprite sheet of `signs`, their images read from the game, cropped to what they show. A sign whose
/// image can't be read is left off (and its name returned in `missing`).
pub fn sprite(files: &GameFiles, signs: &BTreeSet<String>) -> (Sprite, Vec<String>) {
    let mut images = Vec::new();
    let mut missing = Vec::new();
    for sign in signs {
        let path = format!("/material/ui/map/road/road_{sign}.tobj");
        match files.read_texture(&path) {
            Ok(Some(texture)) => match texture.cropped() {
                Some(cropped) => images.push((sign.clone(), cropped)),
                None => missing.push(sign.clone()),
            },
            _ => missing.push(sign.clone()),
        }
    }
    (pack(images), missing)
}

/// Packs images into a sheet: tallest first, in rows across it.
fn pack(mut images: Vec<(String, Texture)>) -> Sprite {
    images.sort_by(|a, b| b.1.height.cmp(&a.1.height).then_with(|| a.0.cmp(&b.0)));
    let mut places = Vec::with_capacity(images.len());
    let (mut x, mut y, mut row_height) = (0, 0, 0);
    for (_, image) in &images {
        if x + image.width > SHEET_WIDTH {
            (x, y, row_height) = (0, y + row_height + GAP, 0);
        }
        places.push((x, y));
        x += image.width + GAP;
        row_height = row_height.max(image.height);
    }
    let height = (y + row_height).max(1);
    let mut sheet = Texture { width: SHEET_WIDTH, height, rgba: vec![0; (SHEET_WIDTH * height * 4) as usize] };
    let mut index = JsonMap::new();
    for ((name, image), &(x, y)) in images.iter().zip(&places) {
        for row in 0..image.height {
            let from = (row * image.width * 4) as usize;
            let to = (((y + row) * SHEET_WIDTH + x) * 4) as usize;
            let length = (image.width * 4) as usize;
            sheet.rgba[to..to + length].copy_from_slice(&image.rgba[from..from + length]);
        }
        index.insert(
            name.clone(),
            json!({ "x": x, "y": y, "width": image.width, "height": image.height, "pixelRatio": PIXEL_RATIO }),
        );
    }
    let signs = images.into_iter().map(|(name, _)| name).collect();
    Sprite { png: sheet.png(), index: Value::Object(index), signs }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32) -> Texture {
        Texture { width, height, rgba: vec![255; (width * height * 4) as usize] }
    }

    #[test]
    fn packs_images_in_rows_apart_and_indexes_them() {
        let wide = SHEET_WIDTH - 10;
        let sprite = pack(vec![("a".into(), image(wide, 20)), ("b".into(), image(30, 10)), ("c".into(), image(5, 30))]);
        // Tallest first: c, then a beside it (5 + 1 + wide fits the row), then b on a new row.
        assert_eq!(sprite.index["c"]["x"], 0);
        assert_eq!(sprite.index["a"]["x"], 6);
        assert_eq!(sprite.index["b"]["y"], 31);
        assert_eq!(sprite.index["b"]["pixelRatio"], PIXEL_RATIO);
        assert_eq!(sprite.signs.len(), 3);
    }
}
