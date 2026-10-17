//! The game's textures, as far as the map needs them: uncompressed 8-bit colour images (the map's road
//! number signs), from the archives' texture entries ([`crate::archive`]) or from `.dds` files extracted
//! from them; cropped to what they show, and written as PNG for the page.

use std::io::Write;

/// An image's pixels: rows top to bottom, 4 bytes a pixel (red, green, blue, alpha; colours as stored,
/// sRGB for the game's UI textures).
#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// DXGI formats read: 8-bit RGBA and BGRA, linear or sRGB (the same bytes).
const R8G8B8A8: [u32; 2] = [28, 29];
const B8G8R8A8: [u32; 2] = [87, 91];

impl Texture {
    /// An image from its top level's bytes in a DXGI `format`, rows `pitch` bytes apart (`None`: packed).
    /// `Err` names what isn't read.
    pub fn from_pixels(
        width: u32,
        height: u32,
        format: u32,
        data: &[u8],
        pitch: Option<usize>,
    ) -> Result<Self, String> {
        let bgra = if B8G8R8A8.contains(&format) {
            true
        } else if R8G8B8A8.contains(&format) {
            false
        } else {
            return Err(format!("texture format {format} (only uncompressed 8-bit colour is read)"));
        };
        let row = width as usize * 4;
        let pitch = pitch.unwrap_or(row);
        if pitch < row || data.len() < pitch * (height as usize - 1) + row {
            return Err(format!("{} bytes for a {width}×{height} image", data.len()));
        }
        let mut rgba = Vec::with_capacity(row * height as usize);
        for y in 0..height as usize {
            for pixel in data[y * pitch..y * pitch + row].chunks_exact(4) {
                let [a, b, c, alpha] = pixel.try_into().unwrap();
                rgba.extend(if bgra { [c, b, a, alpha] } else { [a, b, c, alpha] });
            }
        }
        Ok(Texture { width, height, rgba })
    }

    /// An image from a `.dds` file (as extracted from the archives): its DX10 header's format, or an
    /// older header's 32-bit layout.
    pub fn from_dds(data: &[u8]) -> Result<Self, String> {
        let u32_at = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
        if data.get(..4) != Some(b"DDS ") {
            return Err("not a DDS file".into());
        }
        let (height, width) = (u32_at(12).unwrap_or(0), u32_at(16).unwrap_or(0));
        let fourcc = data.get(84..88).unwrap_or_default();
        if fourcc == b"DX10" {
            let format = u32_at(128).ok_or("short DX10 header")?;
            return Self::from_pixels(width, height, format, data.get(148..).unwrap_or_default(), None);
        }
        // An older header: 32 bits a pixel, blue in the lowest byte (BGRA) or red (RGBA).
        let (bits, red_mask) = (u32_at(88).unwrap_or(0), u32_at(92).unwrap_or(0));
        let format = match (bits, red_mask) {
            (32, 0x00ff_0000) => B8G8R8A8[0],
            (32, 0x0000_00ff) => R8G8B8A8[0],
            _ => return Err(format!("DDS pixel format {:?}", String::from_utf8_lossy(fourcc))),
        };
        Self::from_pixels(width, height, format, data.get(128..).unwrap_or_default(), None)
    }

    /// The image cropped to its visible pixels (any alpha), or `None` if it shows nothing.
    pub fn cropped(&self) -> Option<Texture> {
        let alpha = |x: u32, y: u32| self.rgba[((y * self.width + x) * 4 + 3) as usize];
        let visible: Vec<(u32, u32)> = (0..self.height)
            .flat_map(|y| (0..self.width).map(move |x| (x, y)))
            .filter(|&(x, y)| alpha(x, y) > 0)
            .collect();
        let (x0, x1) = (visible.iter().map(|p| p.0).min()?, visible.iter().map(|p| p.0).max()?);
        let (y0, y1) = (visible.iter().map(|p| p.1).min()?, visible.iter().map(|p| p.1).max()?);
        let (width, height) = (x1 - x0 + 1, y1 - y0 + 1);
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in y0..=y1 {
            let start = ((y * self.width + x0) * 4) as usize;
            rgba.extend(&self.rgba[start..start + (width * 4) as usize]);
        }
        Some(Texture { width, height, rgba })
    }

    /// The image as a PNG file (8-bit RGBA, rows unfiltered, deflated).
    pub fn png(&self) -> Vec<u8> {
        fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
            out.extend((data.len() as u32).to_be_bytes());
            let mut crc = flate2::Crc::new();
            crc.update(kind);
            crc.update(data);
            out.extend(kind);
            out.extend(data);
            out.extend(crc.sum().to_be_bytes());
        }
        let mut header = Vec::with_capacity(13);
        header.extend(self.width.to_be_bytes());
        header.extend(self.height.to_be_bytes());
        header.extend([8, 6, 0, 0, 0]); // 8 bits, RGBA, deflate, no filtering choice, not interlaced
        let mut rows = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        for row in self.rgba.chunks_exact((self.width * 4) as usize) {
            rows.write_all(&[0]).and_then(|()| rows.write_all(row)).expect("writing to memory");
        }
        let rows = rows.finish().expect("writing to memory");
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        chunk(&mut out, b"IHDR", &header);
        chunk(&mut out, b"IDAT", &rows);
        chunk(&mut out, b"IEND", &[]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_bgra_into_rgba_and_crops_to_what_shows() {
        // 3×2, BGRA: a transparent column on the left; blue and red pixels.
        let bgra = [0, 0, 0, 0, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0, 0, 255, 0, 128, 0, 0, 0, 0];
        let texture = Texture::from_pixels(3, 2, 91, &bgra, None).unwrap();
        assert_eq!(&texture.rgba[4..8], &[0, 0, 255, 255]);
        let cropped = texture.cropped().unwrap();
        assert_eq!((cropped.width, cropped.height), (2, 2));
        assert_eq!(&cropped.rgba[..4], &[0, 0, 255, 255]);
        assert!(Texture::from_pixels(3, 2, 71, &bgra, None).is_err(), "block-compressed formats aren't read");
    }

    #[test]
    fn writes_a_png_file() {
        let texture = Texture { width: 1, height: 1, rgba: vec![10, 20, 30, 255] };
        let png = texture.png();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert!(png.ends_with(&[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82]));
    }
}
