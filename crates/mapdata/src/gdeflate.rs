//! GDeflate: the compression the game's archives use for textures (HashFS v2). It is DEFLATE (RFC 1951,
//! with the DEFLATE64 long lengths and distances) with its codes spread over 32 interleaved bit streams,
//! so a GPU can decode them in parallel, in 64 KiB tiles. Written from its specification
//! (draft-uralsky-gdeflate) and the order its reference decoder reads in (NVIDIA's libdeflate fork, for
//! DirectStorage; Apache-2.0, see THIRD_PARTY_NOTICES.md):
//!
//! - A tile stream: codec id 4 and its complement (u8 each), tile count (u16), then a u32 of which bits 2..20
//!   are the last tile's size (0: a whole tile); a u32 per tile (the first: the last tile's packed size;
//!   the others: where each tile starts, from the end of this table); then the tiles.
//! - In a tile, each of the 32 streams starts with one 32-bit word, in order. Codes are read from one
//!   stream at a time, going round them; right after reading from a stream, if it holds fewer than 32
//!   bits it takes the next word of the tile (so the words come in the order they're needed).
//! - Block headers come from stream 0; the code length codes, the code lengths and the block's symbols
//!   one per stream, round the streams. A copy's length takes its stream's turn and its distance the
//!   same stream's next turn (the copy is made then, into room kept for it). After the end-of-block
//!   symbol, one more round makes the copies still waiting. A stored block's length is 16 bits from
//!   stream 0 (no complement, no alignment), its bytes one per stream.

const STREAMS: usize = 32;
/// How far past a tile's end its streams may read before it's taken to be damaged, bytes: a few words each
/// (a sound tile's last round of turns tops each stream up once).
const PAST_END: usize = STREAMS * 4 * 4;
const TILE_SIZE: usize = 65536;
const CODEC_ID: u8 = 4;

/// Unpacks a tile stream that should come to `size` bytes; `None` if it isn't one, or doesn't unpack.
pub fn decompress(data: &[u8], size: usize) -> Option<Vec<u8>> {
    let header = data.get(..8)?;
    if header[0] != CODEC_ID || header[1] != CODEC_ID ^ 0xff {
        return None;
    }
    let tiles = u16::from_le_bytes([header[2], header[3]]) as usize;
    let last_size = (u32::from_le_bytes(header[4..8].try_into().unwrap()) >> 2) & 0x3ffff;
    let table = data.get(8..8 + tiles * 4)?;
    let word = |i: usize| u32::from_le_bytes(table[i * 4..i * 4 + 4].try_into().unwrap()) as usize;
    let body = &data[8 + tiles * 4..];
    // Its tiles can come to no more (the size is the archive's word, not to be trusted with memory).
    if size > tiles * TILE_SIZE {
        return None;
    }
    let mut out = Vec::with_capacity(size);
    for tile in 0..tiles {
        let start = if tile == 0 { 0 } else { word(tile) };
        let end = if tile + 1 < tiles { word(tile + 1) } else { start + word(0) };
        let expected = if tile + 1 == tiles && last_size != 0 { last_size as usize } else { TILE_SIZE };
        let unpacked = Tile::new(body.get(start..end)?).decode(expected)?;
        out.extend(unpacked);
    }
    (out.len() == size).then_some(out)
}

/// The decoder of one tile: its 32 bit streams, the stream whose turn it is, and the copies waiting for
/// their distance (by stream: where they go and how long they are).
struct Tile<'a> {
    input: &'a [u8],
    at: usize,
    bits: [u64; STREAMS],
    count: [u32; STREAMS],
    /// Of each stream's bits, how many (at the top) are filler from past the tile's end.
    filler: [u32; STREAMS],
    /// Whether filler was taken as data: the tile is cut short (a sound one's codes all lie within it).
    past_end: bool,
    turn: usize,
    waiting: [Option<(usize, usize)>; STREAMS],
}

impl<'a> Tile<'a> {
    fn new(input: &'a [u8]) -> Self {
        let mut tile = Tile {
            input,
            at: 0,
            bits: [0; STREAMS],
            count: [0; STREAMS],
            filler: [0; STREAMS],
            past_end: false,
            turn: 0,
            waiting: [None; STREAMS],
        };
        for _ in 0..STREAMS {
            tile.next_turn();
        }
        tile
    }

    /// Tops the current stream up to at least 32 bits (past the tile's end, with zeros).
    fn refill(&mut self) {
        let s = self.turn;
        if self.count[s] < 32 {
            let mut word = [0u8; 4];
            let rest = self.input.get(self.at..).unwrap_or_default();
            let available = rest.len().min(4);
            word[..available].copy_from_slice(&rest[..available]);
            self.filler[s] += (4 - available as u32) * 8;
            self.at += 4;
            self.bits[s] |= u64::from(u32::from_le_bytes(word)) << self.count[s];
            self.count[s] += 32;
        }
    }

    fn next_turn(&mut self) {
        self.refill();
        self.turn = (self.turn + 1) % STREAMS;
    }

    fn take(&mut self, n: u32) -> Option<u32> {
        let s = self.turn;
        if n > self.count[s] {
            return None;
        }
        let value = (self.bits[s] & ((1u64 << n) - 1)) as u32;
        self.bits[s] >>= n;
        // Bits go from the bottom: past the real ones, into the filler.
        let real = self.count[s] - self.filler[s];
        if n > real {
            self.past_end = true;
            self.filler[s] -= n - real;
        }
        self.count[s] -= n;
        Some(value)
    }

    /// The next symbol of `code` from the current stream: the code's bits come first-bit-first.
    fn symbol(&mut self, code: &Huffman) -> Option<u16> {
        let (mut value, mut first, mut index) = (0i32, 0i32, 0i32);
        for length in 1..=15 {
            value |= self.take(1)? as i32;
            let count = i32::from(code.counts[length]);
            if value - first < count {
                return code.symbols.get((index + value - first) as usize).copied();
            }
            index += count;
            first = (first + count) << 1;
            value <<= 1;
        }
        None
    }

    /// Whether the streams have run well past the tile's end: a tile that hasn't ended by then never will
    /// (past the end, its streams read as zeros, which decode as empty blocks for ever).
    fn exhausted(&self) -> bool {
        self.past_end || self.at > self.input.len() + PAST_END
    }

    fn decode(mut self, size: usize) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(size);
        loop {
            if self.exhausted() {
                return None;
            }
            self.turn = 0;
            let last = self.take(1)? == 1;
            let kind = self.take(2)?;
            self.refill();
            match kind {
                0 => {
                    let length = self.take(16)?;
                    if out.len() + length as usize > size {
                        return None;
                    }
                    for _ in 0..length {
                        out.push(self.take(8)? as u8);
                        self.next_turn();
                    }
                    // A block's end takes a round of turns (making waiting copies; none here).
                    for _ in 0..STREAMS {
                        self.next_turn();
                    }
                }
                1 | 2 => {
                    let (literals, distances) = if kind == 2 { self.dynamic_codes()? } else { static_codes() };
                    self.turn = 0;
                    loop {
                        if self.waiting[self.turn].is_some() {
                            self.copy(&mut out, &distances)?;
                        } else {
                            if out.len() > size || self.exhausted() {
                                return None;
                            }
                            let symbol = self.symbol(&literals)?;
                            if symbol < 256 {
                                out.push(symbol as u8);
                            } else if symbol == 256 {
                                break;
                            } else {
                                let slot = usize::from(symbol - 257);
                                let (base, extra) = *LENGTHS.get(slot)?;
                                let length = base as usize + self.take(extra)? as usize;
                                self.waiting[self.turn] = Some((out.len(), length));
                                out.resize(out.len() + length, 0);
                            }
                        }
                        self.next_turn();
                    }
                    // The copies still waiting for their distance, from this stream round.
                    for _ in 0..STREAMS {
                        if self.waiting[self.turn].is_some() {
                            self.copy(&mut out, &distances)?;
                        }
                        self.next_turn();
                    }
                }
                _ => return None,
            }
            if out.len() > size {
                return None;
            }
            if last {
                return (out.len() == size && !self.past_end).then_some(out);
            }
        }
    }

    /// Makes the current stream's waiting copy, its distance read now.
    fn copy(&mut self, out: &mut [u8], distances: &Huffman) -> Option<()> {
        let (to, length) = self.waiting[self.turn].take()?;
        let (base, extra) = *DISTANCES.get(usize::from(self.symbol(distances)?))?;
        let distance = base as usize + self.take(extra)? as usize;
        let from = to.checked_sub(distance)?;
        for i in 0..length {
            out[to + i] = out[from + i];
        }
        Some(())
    }

    /// A dynamic block's codes: its header and code length codes from stream 0 and round the streams, then
    /// the code lengths round the streams.
    fn dynamic_codes(&mut self) -> Option<(Huffman, Huffman)> {
        const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
        let literals = self.take(5)? as usize + 257;
        let distances = self.take(5)? as usize + 1;
        let code_lengths = self.take(4)? as usize + 4;
        self.refill();
        let mut lengths = [0u8; 19];
        for &symbol in &ORDER[..code_lengths] {
            lengths[symbol] = self.take(3)? as u8;
            self.next_turn();
        }
        let precode = Huffman::new(&lengths)?;
        self.turn = 0;
        let mut lens = Vec::with_capacity(literals + distances);
        while lens.len() < literals + distances {
            match self.symbol(&precode)? {
                length @ 0..=15 => lens.push(length as u8),
                16 => {
                    let previous = *lens.last()?;
                    let repeat = 3 + self.take(2)? as usize;
                    lens.extend(std::iter::repeat_n(previous, repeat));
                }
                17 => {
                    let repeat = 3 + self.take(3)? as usize;
                    lens.extend(std::iter::repeat_n(0, repeat));
                }
                _ => {
                    let repeat = 11 + self.take(7)? as usize;
                    lens.extend(std::iter::repeat_n(0, repeat));
                }
            }
            self.next_turn();
        }
        if lens.len() > literals + distances {
            return None;
        }
        Some((Huffman::new(&lens[..literals])?, Huffman::new(&lens[literals..])?))
    }
}

/// A canonical Huffman code: how many codes of each length, and the symbols by code.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Option<Self> {
        let mut counts = [0u16; 16];
        for &length in lengths {
            *counts.get_mut(usize::from(length))? += 1;
        }
        counts[0] = 0;
        let mut offsets = [0u16; 16];
        for length in 1..15 {
            offsets[length + 1] = offsets[length] + counts[length];
        }
        let mut symbols = vec![0u16; lengths.iter().filter(|&&length| length > 0).count()];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length > 0 {
                let slot = &mut offsets[usize::from(length)];
                symbols[usize::from(*slot)] = symbol as u16;
                *slot += 1;
            }
        }
        Some(Huffman { counts, symbols })
    }
}

/// The fixed codes of a static block (RFC 1951 3.2.6).
fn static_codes() -> (Huffman, Huffman) {
    let mut lengths = [0u8; 288];
    lengths[..144].fill(8);
    lengths[144..256].fill(9);
    lengths[256..280].fill(7);
    lengths[280..].fill(8);
    (Huffman::new(&lengths).unwrap(), Huffman::new(&[5; 32]).unwrap())
}

/// Length codes 257 to 285: base and extra bits (285 is DEFLATE64's: 3 plus 16 bits).
const LENGTHS: [(u32, u32); 29] = [
    (3, 0),
    (4, 0),
    (5, 0),
    (6, 0),
    (7, 0),
    (8, 0),
    (9, 0),
    (10, 0),
    (11, 1),
    (13, 1),
    (15, 1),
    (17, 1),
    (19, 2),
    (23, 2),
    (27, 2),
    (31, 2),
    (35, 3),
    (43, 3),
    (51, 3),
    (59, 3),
    (67, 4),
    (83, 4),
    (99, 4),
    (115, 4),
    (131, 5),
    (163, 5),
    (195, 5),
    (227, 5),
    (3, 16),
];

/// Distance codes 0 to 31: base and extra bits (30 and 31 are DEFLATE64's).
const DISTANCES: [(u32, u32); 32] = [
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (5, 1),
    (7, 1),
    (9, 2),
    (13, 2),
    (17, 3),
    (25, 3),
    (33, 4),
    (49, 4),
    (65, 5),
    (97, 5),
    (129, 6),
    (193, 6),
    (257, 7),
    (385, 7),
    (513, 8),
    (769, 8),
    (1025, 9),
    (1537, 9),
    (2049, 10),
    (3073, 10),
    (4097, 11),
    (6145, 11),
    (8193, 12),
    (12289, 12),
    (16385, 13),
    (24577, 13),
    (32769, 14),
    (49153, 14),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A tile holding one stored block of `bytes`: each stream's bits packed into words, laid out in the
    /// order the decoder reads them (its schedule of turns replayed).
    fn stored_tile(bytes: &[u8], last: bool) -> Vec<u8> {
        // Each stream's bits, in the order the decoder takes them, and the decoder's schedule of turns.
        let mut streams: Vec<Vec<(u32, u32)>> = vec![Vec::new(); STREAMS];
        streams[0].push((u32::from(last), 3)); // BFINAL, then BTYPE 00: stored
        streams[0].push((bytes.len() as u32, 16));
        let mut turns = Vec::new(); // (stream, bits it takes this turn), refill checked after each
        for (i, &byte) in bytes.iter().enumerate() {
            streams[i % STREAMS].push((u32::from(byte), 8));
            turns.push(i % STREAMS);
        }
        // Lay the words out by replaying the decoder's reads: initial words, then refills.
        let mut packed: Vec<Vec<u32>> = streams
            .iter()
            .map(|codes| {
                let mut words = Vec::new();
                let (mut acc, mut n) = (0u64, 0u32);
                for &(value, bits) in codes {
                    acc |= u64::from(value) << n;
                    n += bits;
                    while n >= 32 {
                        words.push(acc as u32);
                        acc >>= 32;
                        n -= 32;
                    }
                }
                words.push(acc as u32);
                words.reverse(); // popped from the back
                words
            })
            .collect();
        let mut left = [0u32; STREAMS];
        let mut out = Vec::new();
        let mut read = |stream: usize, left: &mut [u32; STREAMS], out: &mut Vec<u8>| {
            if left[stream] < 32 {
                out.extend(packed[stream].pop().unwrap_or(0).to_le_bytes());
                left[stream] += 32;
            }
        };
        for stream in 0..STREAMS {
            read(stream, &mut left, &mut out);
        }
        // Header (stream 0): 3 bits, refill check, then 16 bits read in the same turn.
        left[0] -= 3;
        read(0, &mut left, &mut out);
        left[0] -= 16;
        for &stream in &turns {
            left[stream] -= 8;
            read(stream, &mut left, &mut out);
        }
        // The round after the block (no copies waiting), from stream 0 on.
        let next = turns.len() % STREAMS;
        for k in 0..STREAMS {
            read((next + k) % STREAMS, &mut left, &mut out);
        }
        out
    }

    fn tile_stream(tile: &[u8], size: usize) -> Vec<u8> {
        let mut data = vec![CODEC_ID, CODEC_ID ^ 0xff, 1, 0];
        data.extend((((size % TILE_SIZE) as u32) << 2 | 1).to_le_bytes());
        data.extend((tile.len() as u32).to_le_bytes());
        data.extend(tile);
        data
    }

    #[test]
    fn unpacks_a_stored_block_spread_over_the_streams() {
        let bytes: Vec<u8> = (0..100u32).map(|i| (i * 7 + 3) as u8).collect();
        let data = tile_stream(&stored_tile(&bytes, true), bytes.len());
        assert_eq!(decompress(&data, bytes.len()), Some(bytes));
    }

    #[test]
    fn refuses_what_isnt_a_tile_stream_or_the_wrong_size() {
        let bytes = [1u8, 2, 3];
        let data = tile_stream(&stored_tile(&bytes, true), bytes.len());
        assert_eq!(decompress(&data, 4), None);
        let mut wrong = data.clone();
        wrong[1] = 0;
        assert_eq!(decompress(&wrong, 3), None);
    }

    /// Decompresses on a thread of its own, so a decoder that never finishes fails the test instead of
    /// hanging it.
    fn decompress_within_a_second(data: Vec<u8>, size: usize) -> Option<Vec<u8>> {
        let (done, result) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = done.send(decompress(&data, size));
        });
        result.recv_timeout(std::time::Duration::from_secs(1)).expect("the decoder didn't finish (or panicked)")
    }

    #[test]
    fn a_damaged_tile_is_refused_not_a_panic_or_a_hang() {
        let bytes: Vec<u8> = (0..100u32).map(|i| i as u8).collect();
        let tile = stored_tile(&bytes, true);
        // Cut short (an archive damaged, or read while being written), into its data. (Its last words are the
        // final round's top-ups, which no code uses: cut there, it still unpacks whole and right.)
        for keep in [0, 3, 17, tile.len() / 2] {
            assert_eq!(decompress_within_a_second(tile_stream(&tile[..keep], bytes.len()), bytes.len()), None);
        }
        assert_eq!(decompress_within_a_second(tile_stream(&tile[..tile.len() - 1], 100), 100), Some(bytes.clone()));
        // Zeros past the end, and a tile of zeros: an endless run of empty blocks, never the last.
        assert_eq!(decompress_within_a_second(tile_stream(&[0; 64], 100), 100), None);
        assert_eq!(decompress_within_a_second(tile_stream(&[0; 4096], TILE_SIZE), TILE_SIZE), None);
    }

    #[test]
    fn a_size_claimed_beyond_the_tiles_is_refused_without_reserving_it() {
        let bytes = [1u8, 2, 3];
        let data = tile_stream(&stored_tile(&bytes, true), bytes.len());
        // One tile can't come to a terabyte: refused (and no terabyte reserved for it first).
        assert_eq!(decompress_within_a_second(data, 1 << 40), None);
    }

    #[test]
    fn decodes_canonical_huffman_codes() {
        // RFC 1951's example: lengths (3, 3, 3, 3, 3, 2, 4, 4) give F = 00, A = 010, ... H = 1111.
        let code = Huffman::new(&[3, 3, 3, 3, 3, 2, 4, 4]).unwrap();
        assert_eq!(code.counts[2..5], [1, 5, 2]);
        assert_eq!(code.symbols, [5, 0, 1, 2, 3, 4, 6, 7]);
    }
}
