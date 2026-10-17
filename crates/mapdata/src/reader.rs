//! A bounds-checked little-endian cursor over a file's bytes.

use std::fmt;

/// Reading went past the end of the data (a truncated file, or a layout we misread).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfBounds {
    /// Where the read started.
    pub offset: usize,
    /// How many bytes it wanted.
    pub wanted: usize,
    /// How long the data is.
    pub len: usize,
}

impl fmt::Display for OutOfBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "read of {} bytes at 0x{:x} is past the end (0x{:x})", self.wanted, self.offset, self.len)
    }
}

impl std::error::Error for OutOfBounds {}

pub type Result<T> = std::result::Result<T, OutOfBounds>;

#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn is_at_end(&self) -> bool {
        self.pos == self.data.len()
    }

    pub fn bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        let error = OutOfBounds { offset: self.pos, wanted: count, len: self.data.len() };
        let end = self.pos.checked_add(count).ok_or(error)?;
        let bytes = self.data.get(self.pos..end).ok_or(error)?;
        self.pos = end;
        Ok(bytes)
    }

    pub fn skip(&mut self, count: usize) -> Result<()> {
        self.bytes(count).map(|_| ())
    }

    /// Skips `count` records of `size` bytes each, for a count read from the file.
    pub fn skip_records(&mut self, count: u32, size: usize) -> Result<()> {
        let total = (count as usize).checked_mul(size).ok_or(OutOfBounds {
            offset: self.pos,
            wanted: usize::MAX,
            len: self.data.len(),
        })?;
        self.skip(total)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.bytes(N)?.try_into().expect("bytes() returns exactly N bytes"))
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    /// A count of following records. Stored as 32 bits, signed in the game's own structures; a negative
    /// one means we are reading the wrong bytes, which the bounds checks then report.
    pub fn count(&mut self) -> Result<u32> {
        self.u32()
    }

    /// `count` 64-bit ids (item uids, tokens).
    pub fn u64s(&mut self, count: u32) -> Result<Vec<u64>> {
        let bytes = self.bytes((count as usize).saturating_mul(8))?;
        Ok(bytes.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap())).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_little_endian_and_tracks_position() {
        let data = [1, 0, 0, 0, 0x34, 0x12, 0, 0, 0x80, 0x3f, 7];
        let mut r = Reader::new(&data);
        assert_eq!(r.u32(), Ok(1));
        assert_eq!(r.u16(), Ok(0x1234));
        assert_eq!(r.f32(), Ok(f32::from_le_bytes([0, 0, 0x80, 0x3f])));
        assert_eq!(r.pos(), 10);
        assert_eq!(r.u8(), Ok(7));
        assert!(r.is_at_end());
    }

    #[test]
    fn refuses_to_read_past_the_end() {
        let data = [0; 6];
        let mut r = Reader::at(&data, 4);
        assert_eq!(r.u32(), Err(OutOfBounds { offset: 4, wanted: 4, len: 6 }));
        assert_eq!(r.pos(), 4, "a failed read moves nothing");
        assert!(r.skip_records(u32::MAX, 0x40).is_err());
        assert!(r.u64s(u32::MAX).is_err());
    }
}
