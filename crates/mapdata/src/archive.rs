//! The game's archives (`.scs`, "HashFS"): files and directories named by the hash of their path
//! ([`crate::cityhash`]), so a file is found by hashing its path, and a directory's entry lists its
//! children's names. Version 2 (the game's since 1.50) and version 1 (older mods) are read; so are mods
//! packed as zip files (as `.zip`, or as `.scs`: many are), their entries found the same way.
//!
//! - Zip: the central directory (from the end-of-directory record) names each file and says where its
//!   local header is; the file's data follows that header, stored or deflated. Directories are what the
//!   paths imply. Encrypted entries and ZIP64 archives aren't read.
//!
//! The layouts follow ts-map's reader (MIT, see THIRD_PARTY_NOTICES.md):
//!
//! - Salt: an archive with a salt (packed by tools that obfuscate a mod) hashes each path with the salt in
//!   front of it, as decimal text: `"32529" + "def"` (worked out on a map mod's version 1 archives, whose
//!   root and files were all found that way). Only version 1 is read salted: no salted version 2 archive
//!   has been seen to check against.
//! - Header: `SCS#`, version (u16), salt (u16), hash method (`CITY`), entry count (u32), then in
//!   version 1 the entry table's offset (u32); in version 2 the entry table's packed size (u32), the
//!   metadata's word count and packed size (u32 each), the entry table's and metadata's offsets (u64
//!   each), both packed with zlib.
//! - Version 1 entries (32 bytes): hash, offset (u64), flags (u32: 1 directory, 2 zlib), crc, size and
//!   stored size (u32).
//! - Version 2 entries (16 bytes): hash, first metadata word and word count (u32, u16), flags (u16:
//!   1 directory). Each metadata word is a kind (top byte) and where the kind's record starts (the rest,
//!   in words). A plain file's or directory's record is four words: stored size (28 bits) and
//!   compression (4 bits: 0 none, 1 zlib, 3 GDeflate, [`crate::gdeflate`]), size (28 bits), one unused,
//!   offset / 16. A texture (`.tobj`) has an image record (kind 1: width - 1 and height - 1, u16 each;
//!   then mip count - 1 in bits 0..4, DXGI format in bits 4..12 and log2 of the rows' alignment in bytes
//!   in bits 20..24) and its pixels in records laid out as a plain file's: its top level (kind 0x82) and the
//!   next (0x83) where it has them, then the rest (0x84, its mip tail: all of it, for a texture of one mip);
//!   rows padded to their alignment, but for the last.
//! - Directory contents: version 1 is text, a name per line, subdirectories marked `*`; version 2 is a
//!   count (u32), each name's length (u8), then the names, subdirectories marked `/`.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::cityhash::city_hash64;
use crate::gdeflate;
use crate::reader::{OutOfBounds, Reader};
use crate::texture::Texture;

/// Where a path's entry is: the hash of the path without its leading slash (the root is "").
pub fn path_hash(path: &str) -> u64 {
    salted_path_hash(path, 0)
}

/// The same in an archive with a salt (0: none): the salt, as decimal text, goes in front of the path.
pub fn salted_path_hash(path: &str, salt: u16) -> u64 {
    let path = path.trim_start_matches('/').trim_end_matches('/');
    if salt == 0 { city_hash64(path.as_bytes()) } else { city_hash64(format!("{salt}{path}").as_bytes()) }
}

pub struct Archive {
    path: PathBuf,
    file: File,
    /// HashFS's version (1 or 2), or 0 for a zip.
    version: u16,
    salt: u16,
    entries: HashMap<u64, Entry>,
    /// A zip's directories (HashFS keeps them as entries).
    listings: HashMap<u64, Listing>,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    directory: bool,
    content: Content,
}

#[derive(Clone, Copy, Debug)]
enum Content {
    Stored(Stored),
    /// A texture: its size, mip count, DXGI format and rows' alignment (bytes), and its pixels.
    Texture {
        width: u32,
        height: u32,
        mips: u32,
        format: u32,
        row_alignment: usize,
        pixels: Stored,
        /// Its top level, where it's kept apart from the mip tail.
        top: Option<Stored>,
    },
    /// Something else (a sound sample, a texture in parts not read): its metadata kind.
    Other(u8),
}

/// A texture's top level as the archive keeps it ([`Archive::read_texture_top`]): its size, DXGI format, rows'
/// alignment (bytes: a block-compressed texture's rows are rows of blocks) and bytes.
#[derive(Clone, Debug)]
pub struct RawTexture {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub row_alignment: usize,
    pub data: Vec<u8>,
}

/// Bytes at `offset`, `stored` of them, `size` once unpacked.
#[derive(Clone, Copy, Debug)]
struct Stored {
    offset: u64,
    size: u32,
    stored: u32,
    compression: Compression,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Compression {
    None,
    Zlib,
    /// Version 2's other method, mostly for textures.
    GDeflate,
    /// A zip's: the offset is its local header's, the data after it, stored or deflated (no zlib header).
    ZipStored,
    ZipDeflate,
    Unknown(u8),
}

/// A directory's children.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listing {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
}

#[derive(Debug)]
pub enum ArchiveError {
    Io(io::Error),
    /// Not a HashFS archive (a zip-packed mod, say).
    NotHashFs,
    UnsupportedVersion(u16),
    /// A salted version 2 archive: none has been seen to check its hashing against.
    Salted(u16),
    UnknownHashMethod([u8; 4]),
    /// The archive's own tables don't add up.
    Corrupt(&'static str),
    OutOfBounds(OutOfBounds),
    /// An entry this reader can't unpack.
    Unsupported {
        hash: u64,
        what: String,
    },
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::NotHashFs => write!(f, "neither a HashFS archive nor a zip"),
            Self::UnsupportedVersion(version) => write!(f, "HashFS version {version} (1 and 2 are read)"),
            Self::Salted(salt) => write!(f, "salted archive ({salt}), not supported"),
            Self::UnknownHashMethod(method) => write!(f, "hash method {:?}", String::from_utf8_lossy(method)),
            Self::Corrupt(what) => write!(f, "corrupt archive: {what}"),
            Self::OutOfBounds(error) => write!(f, "corrupt archive: {error}"),
            Self::Unsupported { hash, what } => write!(f, "entry {hash:016x}: {what}"),
        }
    }
}

impl std::error::Error for ArchiveError {}

impl From<io::Error> for ArchiveError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<OutOfBounds> for ArchiveError {
    fn from(error: OutOfBounds) -> Self {
        Self::OutOfBounds(error)
    }
}

const MAGIC: &[u8; 4] = b"SCS#";
const HASH_METHOD: &[u8; 4] = b"CITY";
const V1_ENTRY_SIZE: usize = 0x20;
const V2_ENTRY_SIZE: usize = 0x10;
/// Version 2 metadata kinds (the top byte of a metadata word).
const IMAGE: u8 = 0x01;
const PLAIN: u8 = 0x80;
const DIRECTORY: u8 = 0x81;
const MIP_0: u8 = 0x82;
const MIP_TAIL: u8 = 0x84;

impl Archive {
    /// Opens an archive and reads its index (the contents are read when asked for).
    pub fn open(path: &Path) -> Result<Self, ArchiveError> {
        let file = File::open(path)?;
        let mut header = [0u8; 0x2C];
        let read = read_up_to(&file, 0, &mut header)?;
        let header = &header[..read];
        if header.starts_with(ZIP_LOCAL) || header.starts_with(ZIP_EMPTY) {
            let (entries, listings) = zip_entries(&file)?;
            return Ok(Self { path: path.to_owned(), file, version: 0, salt: 0, entries, listings });
        }
        let mut r = Reader::new(header);
        if r.bytes(4)? != MAGIC {
            return Err(ArchiveError::NotHashFs);
        }
        let version = r.u16()?;
        let salt = r.u16()?;
        let method: [u8; 4] = r.bytes(4)?.try_into().unwrap();
        if &method != HASH_METHOD {
            return Err(ArchiveError::UnknownHashMethod(method));
        }
        if salt != 0 && version != 1 {
            return Err(ArchiveError::Salted(salt));
        }
        let count = r.u32()? as usize;
        let entries = match version {
            1 => {
                let offset = r.u32()?;
                let table = read_exact(&file, offset.into(), count * V1_ENTRY_SIZE)?;
                v1_entries(&table, count)?
            }
            2 => {
                let entries_packed = r.u32()? as usize;
                let metadata_words = r.u32()? as usize;
                let metadata_packed = r.u32()? as usize;
                let entries_offset = r.u64()?;
                let metadata_offset = r.u64()?;
                let table = inflate(&read_exact(&file, entries_offset, entries_packed)?, count * V2_ENTRY_SIZE)
                    .ok_or(ArchiveError::Corrupt("entry table doesn't unpack"))?;
                let metadata = inflate(&read_exact(&file, metadata_offset, metadata_packed)?, metadata_words * 4)
                    .ok_or(ArchiveError::Corrupt("metadata doesn't unpack"))?;
                v2_entries(&table, count, &metadata)?
            }
            other => return Err(ArchiveError::UnsupportedVersion(other)),
        };
        Ok(Self { path: path.to_owned(), file, version, salt, entries, listings: HashMap::new() })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Where `path`'s entry is in this archive (its salt, if it has one).
    fn hash(&self, path: &str) -> u64 {
        salted_path_hash(path, self.salt)
    }

    /// Whether the archive has an entry (file or directory) at `path`.
    pub fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(&self.hash(path))
    }

    /// A file's contents, or `None` if the archive has no file at `path`.
    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>, ArchiveError> {
        let hash = self.hash(path);
        match self.entries.get(&hash) {
            Some(entry) if !entry.directory => self.contents(hash, entry).map(Some),
            _ => Ok(None),
        }
    }

    /// A texture's image (a `.tobj` path), or `None` if the archive has no texture at `path`.
    pub fn read_texture(&self, path: &str) -> Result<Option<Texture>, ArchiveError> {
        let hash = self.hash(path);
        let Some(entry) = self.entries.get(&hash) else { return Ok(None) };
        let unsupported = |what: String| ArchiveError::Unsupported { hash, what };
        match entry.content {
            Content::Texture { width, height, mips: 1, format, row_alignment, pixels, .. } => {
                let data = self.unpack(hash, pixels)?;
                let pitch = (width as usize * 4).next_multiple_of(row_alignment);
                Texture::from_pixels(width, height, format, &data, Some(pitch)).map(Some).map_err(unsupported)
            }
            Content::Texture { mips, .. } => Err(unsupported(format!("a texture of {mips} mips"))),
            _ => Err(unsupported("not a texture".into())),
        }
    }

    /// A texture's top level as stored (a `.tobj` path), whatever its format: for looking at a block-compressed
    /// texture elsewhere (examples/cab_model writes it as a `.dds`). `None` if the archive has no texture there.
    pub fn read_texture_top(&self, path: &str) -> Result<Option<RawTexture>, ArchiveError> {
        let hash = self.hash(path);
        let Some(entry) = self.entries.get(&hash) else { return Ok(None) };
        match entry.content {
            Content::Texture { width, height, format, row_alignment, pixels, top, .. } => {
                let data = self.unpack(hash, top.unwrap_or(pixels))?;
                Ok(Some(RawTexture { width, height, format, row_alignment, data }))
            }
            _ => Err(ArchiveError::Unsupported { hash, what: "not a texture".into() }),
        }
    }

    /// A directory's children, or `None` if the archive has no directory at `path`.
    pub fn list(&self, path: &str) -> Result<Option<Listing>, ArchiveError> {
        let hash = self.hash(path);
        if self.version == 0 {
            return Ok(self.listings.get(&hash).cloned());
        }
        match self.entries.get(&hash) {
            Some(entry) if entry.directory => {
                let data = self.contents(hash, entry)?;
                let listing = if self.version == 1 { v1_listing(&data) } else { v2_listing(&data)? };
                Ok(Some(listing))
            }
            _ => Ok(None),
        }
    }

    fn contents(&self, hash: u64, entry: &Entry) -> Result<Vec<u8>, ArchiveError> {
        match entry.content {
            Content::Stored(stored) => self.unpack(hash, stored),
            Content::Texture { .. } => Err(ArchiveError::Unsupported { hash, what: "a texture".into() }),
            Content::Other(kind) => {
                Err(ArchiveError::Unsupported { hash, what: format!("not a plain file (kind {kind:#04x})") })
            }
        }
    }

    fn unpack(&self, hash: u64, Stored { offset, size, stored, compression }: Stored) -> Result<Vec<u8>, ArchiveError> {
        // A zip's entry: its data starts after its local header (whose name and extra field vary).
        let offset = match compression {
            Compression::ZipStored | Compression::ZipDeflate => {
                let header = read_exact(&self.file, offset, ZIP_LOCAL_SIZE)?;
                if !header.starts_with(ZIP_LOCAL) {
                    return Err(ArchiveError::Corrupt("zip entry without its local header"));
                }
                let mut r = Reader::new(&header[26..]);
                offset + ZIP_LOCAL_SIZE as u64 + u64::from(r.u16()?) + u64::from(r.u16()?)
            }
            _ => offset,
        };
        // Nothing, said to unpack to nothing: empty, whatever its compression (protected mods mark emptied
        // directories, `/` and `/def` among them, compressed; nothing isn't a stream, and the game reads them as
        // empty: Crni Kal 1.61, 2026-10-24).
        if stored == 0 && size == 0 {
            return Ok(Vec::new());
        }
        let data = read_exact(&self.file, offset, stored as usize)?;
        let unpacked = match compression {
            Compression::None if size == stored => return Ok(data),
            Compression::None => return Err(ArchiveError::Corrupt("unpacked entry's sizes differ")),
            Compression::Zlib => unpack_stream(&data, true, size as usize),
            Compression::GDeflate => gdeflate::decompress(&data, size as usize),
            Compression::ZipStored if size == stored => return Ok(data),
            Compression::ZipStored => return Err(ArchiveError::Corrupt("stored zip entry's sizes differ")),
            Compression::ZipDeflate => unpack_stream(&data, false, size as usize),
            Compression::Unknown(method) => {
                return Err(ArchiveError::Unsupported { hash, what: format!("compression method {method}") });
            }
        };
        unpacked.ok_or(ArchiveError::Corrupt("entry doesn't unpack to its size"))
    }
}

const ZIP_LOCAL: &[u8] = b"PK\x03\x04";
/// An empty zip starts with its end-of-directory record.
const ZIP_EMPTY: &[u8] = b"PK\x05\x06";
const ZIP_CENTRAL: &[u8] = b"PK\x01\x02";
const ZIP_END: &[u8] = b"PK\x05\x06";
/// A local header's fixed part, before its name and extra field.
const ZIP_LOCAL_SIZE: usize = 30;
/// The end-of-directory record's fixed part, and the most its comment can add.
const ZIP_END_SIZE: usize = 22;
const ZIP_MAX_COMMENT: usize = 0xFFFF;

/// A zip's files (by their paths' hashes) and the directories their paths imply, from its central directory.
/// A zip's files and its directories, each by its path's hash.
type ZipIndex = (HashMap<u64, Entry>, HashMap<u64, Listing>);

fn zip_entries(file: &File) -> Result<ZipIndex, ArchiveError> {
    let length = file.metadata()?.len();
    let tail_length = (length as usize).min(ZIP_END_SIZE + ZIP_MAX_COMMENT);
    let tail = read_exact(file, length - tail_length as u64, tail_length)?;
    let end = (0..=tail_length.saturating_sub(ZIP_END_SIZE))
        .rev()
        .find(|&at| tail[at..].starts_with(ZIP_END))
        .ok_or(ArchiveError::Corrupt("zip without its end-of-directory record"))?;
    let mut r = Reader::new(&tail[end + 10..]);
    let count = r.u16()? as usize;
    let size = r.u32()?;
    let offset = r.u32()?;
    if count == 0xFFFF || size == u32::MAX || offset == u32::MAX {
        return Err(ArchiveError::Unsupported { hash: 0, what: "a ZIP64 archive".into() });
    }
    let directory = read_exact(file, offset.into(), size as usize)?;
    let mut r = Reader::new(&directory);
    let mut entries = HashMap::with_capacity(count);
    let mut listings: HashMap<String, (BTreeSet<String>, BTreeSet<String>)> = HashMap::new();
    listings.entry(String::new()).or_default();
    for _ in 0..count {
        if r.bytes(4)? != ZIP_CENTRAL {
            return Err(ArchiveError::Corrupt("zip directory entry without its signature"));
        }
        r.bytes(4)?;
        // The flags are read past, the "encrypted" one (bit 0) too, as the game does: protected mods set it on
        // entries that aren't encrypted, to stop unpackers. A truly encrypted entry doesn't unpack to its size,
        // and is refused then.
        r.u16()?;
        let method = r.u16()?;
        r.bytes(8)?;
        let stored = r.u32()?;
        let unpacked = r.u32()?;
        let name_length = r.u16()? as usize;
        let extra_length = r.u16()? as usize;
        let comment_length = r.u16()? as usize;
        r.bytes(8)?;
        let local = r.u32()?;
        let name = String::from_utf8_lossy(r.bytes(name_length)?).replace('\\', "/");
        r.bytes(extra_length + comment_length)?;
        let path = name.trim_start_matches('/').to_owned();
        // The directories the path is in (and, ending in a slash, itself one).
        let directory = path.ends_with('/');
        let path = path.trim_end_matches('/').to_owned();
        let mut parent = String::new();
        let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
        for (index, part) in parts.iter().enumerate() {
            let last = index + 1 == parts.len();
            let (dirs, files) = listings.entry(parent.clone()).or_default();
            if last && !directory {
                files.insert((*part).to_owned())
            } else {
                dirs.insert((*part).to_owned())
            };
            parent = if parent.is_empty() { (*part).to_owned() } else { format!("{parent}/{part}") };
            if !last || directory {
                listings.entry(parent.clone()).or_default();
            }
        }
        if directory || path.is_empty() {
            continue;
        }
        let compression = match method {
            0 => Compression::ZipStored,
            8 => Compression::ZipDeflate,
            other => Compression::Unknown(other as u8),
        };
        let content = Content::Stored(Stored { offset: local.into(), size: unpacked, stored, compression });
        entries.insert(salted_path_hash(&path, 0), Entry { directory: false, content });
    }
    let listings = listings
        .into_iter()
        .map(|(path, (dirs, files))| {
            let listing = Listing { dirs: dirs.into_iter().collect(), files: files.into_iter().collect() };
            (salted_path_hash(&path, 0), listing)
        })
        .collect();
    Ok((entries, listings))
}

/// Unpacks an entry's DEFLATE stream (zlib's format with `zlib`, else a zip's raw one), said to come to
/// `claimed` bytes. A stream that unpacks whole to its end is taken whatever size it was said to have: protected
/// mods give their entries wrong sizes to stop unpackers, and the game reads them. What vouches for the data
/// is the stream itself: zlib's own checksum has to match, and a stream that stops short, or would unpack to
/// more than DEFLATE can, is refused.
fn unpack_stream(data: &[u8], zlib: bool, claimed: usize) -> Option<Vec<u8>> {
    use flate2::{Decompress, FlushDecompress, Status};
    // DEFLATE packs at most about 1032 to 1.
    let most = data.len().saturating_mul(1032).saturating_add(64);
    let mut out = Vec::with_capacity(inflated_capacity(data.len(), claimed).max(64));
    let mut stream = Decompress::new(zlib);
    loop {
        if out.len() == out.capacity() {
            if out.len() >= most {
                return None;
            }
            // Doubling, by 64 KiB at the least, but never past what it could unpack to (a small stream's room
            // left can be under 64 KiB: a clamp's least above its most panics).
            out.reserve(out.len().max(64 * 1024).min(most - out.len()));
        }
        let (before_in, before_out) = (stream.total_in(), stream.total_out());
        let read = usize::try_from(before_in).ok()?;
        // Unpacked a piece at a time: `Finish` would need all the room at once.
        match stream.decompress_vec(data.get(read..)?, &mut out, FlushDecompress::None).ok()? {
            Status::StreamEnd => return Some(out),
            // Room left to unpack into, and nothing more came: the stream stops short.
            _ if stream.total_in() == before_in && stream.total_out() == before_out && out.len() < out.capacity() => {
                return None;
            }
            _ => {}
        }
    }
}

fn v1_entries(table: &[u8], count: usize) -> Result<HashMap<u64, Entry>, ArchiveError> {
    let mut r = Reader::new(table);
    let mut entries = HashMap::with_capacity(count);
    for _ in 0..count {
        let hash = r.u64()?;
        let offset = r.u64()?;
        let flags = r.u32()?;
        let _crc = r.u32()?;
        let size = r.u32()?;
        let stored = r.u32()?;
        let compression = if flags & 2 != 0 { Compression::Zlib } else { Compression::None };
        let content = Content::Stored(Stored { offset, size, stored, compression });
        entries.insert(hash, Entry { directory: flags & 1 != 0, content });
    }
    Ok(entries)
}

fn v2_entries(table: &[u8], count: usize, metadata: &[u8]) -> Result<HashMap<u64, Entry>, ArchiveError> {
    let word = |index: usize| -> Result<u32, ArchiveError> { Ok(Reader::at(metadata, index * 4).u32()?) };
    let mut r = Reader::new(table);
    let mut entries = HashMap::with_capacity(count);
    for _ in 0..count {
        let hash = r.u64()?;
        let first = r.u32()? as usize;
        let words = r.u16()? as usize;
        let flags = r.u16()?;
        // A record laid out as a plain file's.
        let stored = |record: usize| -> Result<Stored, ArchiveError> {
            let (packed, size, offset) = (word(record)?, word(record + 1)?, word(record + 3)?);
            let compression = match packed >> 28 {
                0 => Compression::None,
                1 => Compression::Zlib,
                3 => Compression::GDeflate,
                other => Compression::Unknown(other as u8),
            };
            Ok(Stored {
                offset: u64::from(offset) * 16,
                size: size & 0x0FFF_FFFF,
                stored: packed & 0x0FFF_FFFF,
                compression,
            })
        };
        let (mut content, mut image, mut mip_tail, mut top) = (None, None, None, None);
        for index in first..first + words {
            let header = word(index)?;
            let (kind, record) = ((header >> 24) as u8, (header & 0x00FF_FFFF) as usize);
            match kind {
                PLAIN | DIRECTORY => {
                    content = Some(Content::Stored(stored(record)?));
                    break;
                }
                IMAGE => image = Some((word(record)?, word(record + 1)?)),
                MIP_0 => top = Some(stored(record)?),
                MIP_TAIL => mip_tail = Some(stored(record)?),
                _ => {}
            }
            content.get_or_insert(Content::Other(kind));
        }
        if let (Some((size, layout)), Some(pixels), None | Some(Content::Other(_))) = (image, mip_tail, content) {
            content = Some(Content::Texture {
                width: (size & 0xFFFF) + 1,
                height: (size >> 16) + 1,
                mips: (layout & 0xF) + 1,
                format: (layout >> 4) & 0xFF,
                row_alignment: 1 << ((layout >> 20) & 0xF),
                pixels,
                top,
            });
        }
        let content = content.ok_or(ArchiveError::Corrupt("entry without metadata"))?;
        entries.insert(hash, Entry { directory: flags & 1 != 0, content });
    }
    Ok(entries)
}

fn v1_listing(data: &[u8]) -> Listing {
    let mut listing = Listing::default();
    for line in String::from_utf8_lossy(data).lines().filter(|line| !line.is_empty()) {
        match line.strip_prefix('*') {
            Some(dir) => listing.dirs.push(dir.to_owned()),
            None => listing.files.push(line.to_owned()),
        }
    }
    listing
}

fn v2_listing(data: &[u8]) -> Result<Listing, ArchiveError> {
    let mut r = Reader::new(data);
    let count = r.u32()? as usize;
    let lengths = r.bytes(count)?.to_vec();
    let mut listing = Listing::default();
    for length in lengths {
        let name = String::from_utf8_lossy(r.bytes(length.into())?).into_owned();
        match name.strip_prefix('/') {
            Some(dir) => listing.dirs.push(dir.to_owned()),
            None => listing.files.push(name),
        }
    }
    Ok(listing)
}

/// Unpacks zlib data that should come to exactly `size` bytes.
fn inflate(data: &[u8], size: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(inflated_capacity(data.len(), size));
    flate2::read::ZlibDecoder::new(data).take(size as u64 + 1).read_to_end(&mut out).ok()?;
    (out.len() == size).then_some(out)
}

/// How much to reserve for zlib data of `packed` bytes said to unpack to `claimed`: no more than it can
/// (DEFLATE packs at most about 1032 to 1), whatever a damaged header claims.
pub(crate) fn inflated_capacity(packed: usize, claimed: usize) -> usize {
    claimed.min(packed.saturating_mul(1032).saturating_add(64))
}

fn read_exact(file: &File, offset: u64, len: usize) -> io::Result<Vec<u8>> {
    // Only what the file holds (a damaged table's length is the archive's word, not to be trusted with
    // memory).
    let (end, file_len) = (offset.checked_add(len as u64), file.metadata()?.len());
    if end.is_none_or(|end| end > file_len) {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "archive ends early"));
    }
    let mut buf = vec![0u8; len];
    if read_up_to(file, offset, &mut buf)? < len {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "archive ends early"));
    }
    Ok(buf)
}

/// Reads at `offset` without moving a shared cursor (so threads can read one archive at once), until
/// `buf` is full or the file ends. Returns how much was read.
fn read_up_to(file: &File, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let read = read_at(file, offset + filled as u64, &mut buf[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

#[cfg(windows)]
fn read_at(file: &File, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buf, offset)
}

#[cfg(unix)]
fn read_at(file: &File, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, offset)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::test_dir::TempDir;

    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn raw_deflate(data: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn an_entry_whose_size_lies_is_read_whole_if_its_stream_is() {
        // Protected mods' trick: the size said is wrong, the data whole (the game reads it).
        let text: Vec<u8> = b"SiiNunit\n{\n city : city.calais {}\n}\n".repeat(200);
        for claimed in [0, 3, text.len() - 1, text.len(), text.len() + 1, 1 << 30] {
            assert_eq!(unpack_stream(&zlib(&text), true, claimed).as_deref(), Some(&text[..]));
            assert_eq!(unpack_stream(&raw_deflate(&text), false, claimed).as_deref(), Some(&text[..]));
        }
        // A stream cut short is refused, whatever its size says.
        let packed = zlib(&text);
        assert_eq!(unpack_stream(&packed[..packed.len() / 2], true, text.len()), None);
        let packed = raw_deflate(&text);
        assert_eq!(unpack_stream(&packed[..packed.len() / 2], false, text.len()), None);
        // So is zlib data whose checksum doesn't match: damaged, not just mislabelled.
        let mut damaged = zlib(&text);
        let last = damaged.len() - 1;
        damaged[last] ^= 0xFF;
        assert_eq!(unpack_stream(&damaged, true, text.len()), None);
        // And what isn't DEFLATE at all.
        assert_eq!(unpack_stream(b"not packed at all", true, 17), None);
    }

    #[test]
    fn a_small_stream_that_unpacks_big_is_read_whole_whatever_its_size_says() {
        // A few dozen bytes unpacking to tens of kilobytes, said to be smaller than it unpacks to: the room left
        // under what it could unpack to fell below the 64 KiB step, and the step's clamp panicked (2.0.3, a
        // player's map: "min > max. min = 65536, max = 42304"), stopping every profile's map.
        let zeros = vec![0u8; 40_000];
        for claimed in [0, 3, 64, 1000, zeros.len() - 1] {
            assert_eq!(unpack_stream(&zlib(&zeros), true, claimed).as_deref(), Some(&zeros[..]));
            assert_eq!(unpack_stream(&raw_deflate(&zeros), false, claimed).as_deref(), Some(&zeros[..]));
        }
    }

    #[test]
    fn a_damaged_size_is_an_error_not_an_abort() {
        // Said to unpack to a terabyte: refused, without reserving a terabyte first.
        assert_eq!(inflate(&zlib(b"abc"), 1 << 40), None);
        assert_eq!(inflate(&zlib(b"abc"), 3), Some(b"abc".to_vec()));
        // An entry said to unpack to a terabyte is refused only if its stream doesn't unpack whole.
        assert_eq!(unpack_stream(&zlib(b"abc"), true, 1 << 40), Some(b"abc".to_vec()));
        // A table said to lie past the file's end.
        let dir = TempDir::new();
        let path = dir.path.join("small.scs");
        std::fs::write(&path, [0u8; 16]).unwrap();
        let file = File::open(&path).unwrap();
        assert!(read_exact(&file, 8, 1 << 40).is_err());
        assert!(read_exact(&file, u64::MAX - 4, 16).is_err());
        assert_eq!(read_exact(&file, 8, 8).unwrap(), [0u8; 8]);
    }

    fn v2_dir(names: &[&str]) -> Vec<u8> {
        let mut out = (names.len() as u32).to_le_bytes().to_vec();
        out.extend(names.iter().map(|name| name.len() as u8));
        out.extend(names.iter().flat_map(|name| name.bytes()));
        out
    }

    /// A version 2 archive of (path, contents, directory, packed) entries.
    fn build_v2(entries: &[(&str, Vec<u8>, bool, bool)]) -> Vec<u8> {
        const HEADER: usize = 0x2C;
        let mut data = vec![0u8; HEADER];
        // One metadata word per entry; their records (four words each) after all of them.
        let (mut table, mut words, mut records) = (Vec::new(), Vec::new(), Vec::new());
        for (index, (path, contents, directory, packed)) in entries.iter().enumerate() {
            data.resize(data.len().next_multiple_of(16), 0);
            let stored = if *packed { zlib(contents) } else { contents.clone() };
            records.extend(((u32::from(*packed) << 28) | stored.len() as u32).to_le_bytes());
            records.extend((contents.len() as u32).to_le_bytes());
            records.extend(0u32.to_le_bytes());
            records.extend(((data.len() / 16) as u32).to_le_bytes());
            data.extend(&stored);
            let kind = if *directory { DIRECTORY } else { PLAIN };
            words.extend(((u32::from(kind) << 24) | (entries.len() + index * 4) as u32).to_le_bytes());
            table.extend(path_hash(path).to_le_bytes());
            table.extend((index as u32).to_le_bytes());
            table.extend(1u16.to_le_bytes());
            table.extend(u16::from(*directory).to_le_bytes());
        }
        words.extend(records);
        let (table, metadata) = (zlib(&table), zlib(&words));
        let (table_offset, metadata_offset) = (data.len() as u64, (data.len() + table.len()) as u64);
        data.extend(&table);
        data.extend(&metadata);
        let mut header = Vec::new();
        header.extend(MAGIC);
        header.extend(2u16.to_le_bytes());
        header.extend(0u16.to_le_bytes());
        header.extend(HASH_METHOD);
        header.extend((entries.len() as u32).to_le_bytes());
        header.extend((table.len() as u32).to_le_bytes());
        header.extend(((words.len() / 4) as u32).to_le_bytes());
        header.extend((metadata.len() as u32).to_le_bytes());
        header.extend(table_offset.to_le_bytes());
        header.extend(metadata_offset.to_le_bytes());
        data[..HEADER].copy_from_slice(&header);
        data
    }

    fn open(bytes: &[u8]) -> (Archive, TempDir) {
        let dir = TempDir::new();
        let path = dir.path.join("test.scs");
        std::fs::write(&path, bytes).unwrap();
        (Archive::open(&path).unwrap(), dir)
    }

    #[test]
    fn reads_version_2_files_and_directories() {
        let def = b"SiiNunit\n{\n}\n".repeat(20);
        let (archive, _dir) = open(&build_v2(&[
            ("", v2_dir(&["/def", "readme.txt"]), true, false),
            ("def", v2_dir(&["city.sii"]), true, true),
            ("def/city.sii", def.clone(), false, true),
            ("readme.txt", b"hello".to_vec(), false, false),
        ]));
        assert_eq!(archive.len(), 4);
        let root = archive.list("/").unwrap().unwrap();
        assert_eq!((root.dirs, root.files), (vec!["def".to_owned()], vec!["readme.txt".to_owned()]));
        assert_eq!(archive.list("/def").unwrap().unwrap().files, ["city.sii"]);
        assert_eq!(archive.read("/def/city.sii").unwrap(), Some(def));
        assert_eq!(archive.read("readme.txt").unwrap(), Some(b"hello".to_vec()));
        // Missing paths, and a directory read as a file (or the other way round), are simply not there.
        assert_eq!(archive.read("/def/missing.sii").unwrap(), None);
        assert_eq!(archive.read("/def").unwrap(), None);
        assert!(archive.list("/def/city.sii").unwrap().is_none());
    }

    #[test]
    fn reads_version_1() {
        let (archive, _dir) = open(&build_v1(0));
        assert_eq!(archive.read("/version.txt").unwrap(), Some(b"one\ntwo".to_vec()));
        let root = archive.list("").unwrap().unwrap();
        assert_eq!((root.dirs, root.files), (vec!["map".to_owned()], vec!["version.txt".to_owned()]));
    }

    #[test]
    fn an_emptied_directory_marked_packed_is_empty() {
        // Crni Kal 1.61's trick: `/` and `/def` emptied, marked packed (nothing isn't a stream); the folders
        // deeper are found by their own paths.
        let listing = b"curve_model.crni_kal_indi.sii".to_vec();
        let mut data = vec![0u8; 0x14];
        let dir_offset = data.len() as u64;
        data.extend(&listing);
        let table_offset = data.len() as u32;
        let entries = [("", 0, 0), ("def", 0, 0), ("def/world", dir_offset, listing.len())];
        for (path, offset, size) in entries {
            data.extend(salted_path_hash(path, 0).to_le_bytes());
            data.extend(offset.to_le_bytes());
            data.extend((if size == 0 { 3u32 } else { 1 }).to_le_bytes()); // a directory, packed if empty
            data.extend(0u32.to_le_bytes());
            data.extend((size as u32).to_le_bytes());
            data.extend((size as u32).to_le_bytes());
        }
        let mut header = Vec::new();
        header.extend(MAGIC);
        header.extend(1u16.to_le_bytes());
        header.extend(0u16.to_le_bytes());
        header.extend(HASH_METHOD);
        header.extend((entries.len() as u32).to_le_bytes());
        header.extend(table_offset.to_le_bytes());
        data[..header.len()].copy_from_slice(&header);
        let (archive, _dir) = open(&data);
        for empty in ["", "def"] {
            let listing = archive.list(empty).unwrap().unwrap();
            assert!(listing.dirs.is_empty() && listing.files.is_empty(), "{empty:?}");
        }
        assert_eq!(archive.list("def/world").unwrap().unwrap().files, ["curve_model.crni_kal_indi.sii"]);
    }

    #[test]
    fn reads_a_salted_version_1_archive() {
        // Paths hashed with the salt in front: found by their names all the same.
        let (archive, _dir) = open(&build_v1(32529));
        assert_eq!(archive.read("/version.txt").unwrap(), Some(b"one\ntwo".to_vec()));
        assert!(archive.list("").unwrap().is_some());
        assert_ne!(salted_path_hash("def", 32529), path_hash("def"));
        assert_eq!(salted_path_hash("/def/", 32529), salted_path_hash("def", 32529));
    }

    /// A version 1 archive with a file (`version.txt`, packed) and the root listing it and `map`, its paths
    /// hashed with `salt`.
    fn build_v1(salt: u16) -> Vec<u8> {
        let contents = b"one\ntwo".to_vec();
        let listing = b"*map\nversion.txt\n".to_vec();
        let mut data = vec![0u8; 0x14];
        let file_offset = data.len() as u64;
        let packed = zlib(&contents);
        data.extend(&packed);
        let dir_offset = data.len() as u64;
        data.extend(&listing);
        let table_offset = data.len() as u32;
        for (path, offset, flags, size, stored) in [
            ("version.txt", file_offset, 2u32, contents.len(), packed.len()),
            ("", dir_offset, 1, listing.len(), listing.len()),
        ] {
            data.extend(salted_path_hash(path, salt).to_le_bytes());
            data.extend(offset.to_le_bytes());
            data.extend(flags.to_le_bytes());
            data.extend(0u32.to_le_bytes());
            data.extend((size as u32).to_le_bytes());
            data.extend((stored as u32).to_le_bytes());
        }
        let mut header = Vec::new();
        header.extend(MAGIC);
        header.extend(1u16.to_le_bytes());
        header.extend(salt.to_le_bytes());
        header.extend(HASH_METHOD);
        header.extend(2u32.to_le_bytes());
        header.extend(table_offset.to_le_bytes());
        data[..header.len()].copy_from_slice(&header);
        data
    }

    /// A zip of (path, contents, deflated) entries, as a mod's: local headers and data, then the central
    /// directory and its end record.
    fn build_zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        use std::io::Write;
        let (mut data, mut directory) = (Vec::new(), Vec::new());
        for (path, contents, deflated) in entries {
            let stored = if *deflated {
                let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(contents).unwrap();
                encoder.finish().unwrap()
            } else {
                contents.to_vec()
            };
            let method: u16 = if *deflated { 8 } else { 0 };
            let local = data.len() as u32;
            data.extend(b"PK");
            data.extend([20u8, 0, 0, 0]);
            data.extend(method.to_le_bytes());
            data.extend([0u8; 8]);
            data.extend((stored.len() as u32).to_le_bytes());
            data.extend((contents.len() as u32).to_le_bytes());
            data.extend((path.len() as u16).to_le_bytes());
            data.extend(3u16.to_le_bytes());
            data.extend(path.as_bytes());
            data.extend(b"abc");
            data.extend(&stored);
            directory.extend(b"PK");
            directory.extend([20u8, 0, 20, 0, 0, 0]);
            directory.extend(method.to_le_bytes());
            directory.extend([0u8; 8]);
            directory.extend((stored.len() as u32).to_le_bytes());
            directory.extend((contents.len() as u32).to_le_bytes());
            directory.extend((path.len() as u16).to_le_bytes());
            directory.extend([0u8; 12]);
            directory.extend(local.to_le_bytes());
            directory.extend(path.as_bytes());
        }
        let offset = data.len() as u32;
        data.extend(&directory);
        data.extend(b"PK");
        data.extend([0u8; 4]);
        data.extend((entries.len() as u16).to_le_bytes());
        data.extend((entries.len() as u16).to_le_bytes());
        data.extend((directory.len() as u32).to_le_bytes());
        data.extend(offset.to_le_bytes());
        data.extend(0u16.to_le_bytes());
        data
    }

    #[test]
    fn reads_a_zip_mod() {
        let def = b"SiiNunit
{
}
"
        .repeat(20);
        let (archive, _dir) = open(&build_zip(&[
            ("manifest.sii", b"SiiNunit { }", false),
            ("def/world/road_look.mod.sii", &def, true),
            ("map/mymap/", b"", false),
            ("map/mymap/sec+0000+0000.base", b"sector", false),
        ]));
        assert_eq!(archive.read("/manifest.sii").unwrap().as_deref(), Some(&b"SiiNunit { }"[..]));
        assert_eq!(archive.read("def/world/road_look.mod.sii").unwrap(), Some(def));
        assert!(archive.contains("map/mymap/sec+0000+0000.base"));
        let root = archive.list("").unwrap().unwrap();
        assert_eq!(
            (root.dirs, root.files),
            (vec!["def".to_owned(), "map".to_owned()], vec!["manifest.sii".to_owned()])
        );
        assert_eq!(archive.list("map").unwrap().unwrap().dirs, ["mymap"]);
        assert_eq!(archive.list("/def/world/").unwrap().unwrap().files, ["road_look.mod.sii"]);
        assert_eq!(archive.read("missing.sii").unwrap(), None);
    }

    #[test]
    fn reads_a_protected_zip_mod_whose_entries_claim_to_be_encrypted() {
        let def = b"SiiNunit { company : acc {} }".repeat(10);
        let mut zip = build_zip(&[("def/company/acc.sui", &def, true), ("manifest.sii", b"SiiNunit { }", false)]);
        // Set the "encrypted" flag on every entry, local and central, as protected mods do.
        for signature in [&b"PK\x03\x04"[..], &b"PK\x01\x02"[..]] {
            let flag_at = if signature == b"PK\x03\x04" { 6 } else { 8 };
            let starts: Vec<usize> =
                zip.windows(4).enumerate().filter(|(_, w)| *w == signature).map(|(i, _)| i).collect();
            for start in starts {
                zip[start + flag_at] |= 1;
            }
        }
        let (archive, _dir) = open(&zip);
        assert_eq!(archive.read("def/company/acc.sui").unwrap(), Some(def));
        assert_eq!(archive.read("manifest.sii").unwrap().as_deref(), Some(&b"SiiNunit { }"[..]));
    }

    #[test]
    fn refuses_a_zip_entry_that_is_truly_encrypted() {
        let contents = b"SiiNunit { }".repeat(10);
        let mut zip = build_zip(&[("def/secret.sii", &contents, true)]);
        // Scrambled data, as encryption leaves it: it doesn't unpack to the entry's size.
        let data_at = 30 + "def/secret.sii".len() + 3;
        for byte in &mut zip[data_at..data_at + 8] {
            *byte ^= 0x5A;
        }
        let (archive, _dir) = open(&zip);
        assert!(archive.read("def/secret.sii").is_err());
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        let dir = TempDir::new();
        let path = dir.path.join("mod.scs");
        std::fs::write(&path, b"PNG? not an archive").unwrap();
        assert!(matches!(Archive::open(&path), Err(ArchiveError::NotHashFs)));
        // A broken zip: no end-of-directory record.
        std::fs::write(&path, b"PK\x03\x04 a zip cut short").unwrap();
        assert!(matches!(Archive::open(&path), Err(ArchiveError::Corrupt(_))));
        let mut salted = build_v2(&[("", v2_dir(&[]), true, false)]);
        salted[6] = 7;
        std::fs::write(&path, &salted).unwrap();
        assert!(matches!(Archive::open(&path), Err(ArchiveError::Salted(7))));
    }
}
