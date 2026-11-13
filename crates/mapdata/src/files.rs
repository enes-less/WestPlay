//! The game's file system: archives mounted on top of each other, a later one's files replacing an
//! earlier one's at the same path (the base game first, then its DLCs, then mods). Each is the game's
//! own `.scs` archive ([`crate::archive`]), or a folder extracted from one (for development).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::archive::{Archive, ArchiveError};
use crate::texture::Texture;

/// One mounted archive.
pub enum Mount {
    Archive(Archive),
    Folder(PathBuf),
}

impl Mount {
    fn name(&self) -> &Path {
        match self {
            Self::Archive(archive) => archive.path(),
            Self::Folder(path) => path,
        }
    }
}

/// A path that couldn't be read from a mount (or, with no path, a mount that couldn't be opened).
#[derive(Debug)]
pub struct FileError {
    pub path: String,
    pub mount: PathBuf,
    pub error: ArchiveError,
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // No path: the mount itself couldn't be opened.
        if self.path.is_empty() {
            write!(f, "{}: {}", self.mount.display(), self.error)
        } else {
            write!(f, "{} in {}: {}", self.path, self.mount.display(), self.error)
        }
    }
}

impl std::error::Error for FileError {}

/// The base game's archives, in the order they're mounted; DLCs follow ([`GameFiles::install`]). Its trucks'
/// interiors and screens (`base_vehicle`) and its default settings (`base_cfg`) included.
pub const BASE_ARCHIVES: [&str; 8] =
    ["base", "base_map", "base_share", "base_vehicle", "base_cfg", "core", "def", "locale"];

pub struct GameFiles {
    /// Earliest first.
    mounts: Vec<Mount>,
    /// The files that couldn't be read or understood, which what's made from them is made without
    /// ([`Self::note_unread`]): by path, why.
    unread: std::sync::Mutex<BTreeMap<String, String>>,
}

/// The files a map was made without, as they couldn't be read or understood ([`GameFiles::note_unread`]): how many,
/// and the first few by path (each its mount and why).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Unread {
    pub count: usize,
    pub first: Vec<String>,
}

/// How many of the files a map was made without are named ([`Unread::first`]).
const UNREAD_NAMED: usize = 10;

impl GameFiles {
    /// `mounts` in mount order: a file in a later one replaces the same path in an earlier one.
    pub fn new(mounts: Vec<Mount>) -> Self {
        Self { mounts, unread: std::sync::Mutex::default() }
    }

    /// Notes that `path` couldn't be read or understood (`why`), so what it holds is left out: a definition, a
    /// prefab's descriptor. They were skipped without a word: a mod's road looks or junctions gone, nothing said
    /// (the review of 2026-11-11, M9).
    pub fn note_unread(&self, path: &str, why: &dyn fmt::Display) {
        let from = self.source(path).map_or_else(|| "?".to_owned(), |mount| mount.display().to_string());
        let mut unread = self.unread.lock().unwrap_or_else(|e| e.into_inner());
        unread.entry(path.to_owned()).or_insert_with(|| format!("{path} in {from}: {why}"));
    }

    /// The files noted unread so far ([`Self::note_unread`]).
    pub fn unread(&self) -> Unread {
        let unread = self.unread.lock().unwrap_or_else(|e| e.into_inner());
        Unread { count: unread.len(), first: unread.values().take(UNREAD_NAMED).cloned().collect() }
    }

    /// The game as installed in `dir` (the folder with `base.scs`): the base game's archives, then
    /// every DLC's (`dlc_*.scs`), alphabetically. (Which DLCs the player owns is still to be checked:
    /// Steam keeps only those, but other stores may differ.)
    pub fn install(dir: &Path) -> Result<Self, FileError> {
        Self::mounted(dir, ".scs", |path| {
            Archive::open(path).map(Mount::Archive).map_err(|error| FileError {
                path: String::new(),
                mount: path.to_owned(),
                error,
            })
        })
    }

    /// The game in `dir`: its archives if it has them (the install folder, with `base.scs`), otherwise
    /// folders extracted from them ([`Self::extracted`]).
    pub fn open(dir: &Path) -> Result<Self, FileError> {
        if dir.join("base.scs").is_file() { Self::install(dir) } else { Self::extracted(dir) }
    }

    /// The same, from archives extracted into folders named after them (`base/`, `dlc_fr/`…), for
    /// development. A missing base archive's folder is skipped.
    pub fn extracted(dir: &Path) -> Result<Self, FileError> {
        Self::mounted(dir, "", |path| Ok(Mount::Folder(path.to_owned())))
    }

    fn mounted(
        dir: &Path,
        extension: &str,
        mount: impl Fn(&Path) -> Result<Mount, FileError>,
    ) -> Result<Self, FileError> {
        let io_error = |error| FileError { path: String::new(), mount: dir.to_owned(), error: ArchiveError::Io(error) };
        let mut dlcs: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(io_error)?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else { return false };
                let kind = if extension.is_empty() { path.is_dir() } else { path.is_file() };
                name.starts_with("dlc_") && name.ends_with(extension) && kind
            })
            .collect();
        dlcs.sort();
        let base = BASE_ARCHIVES.iter().map(|name| dir.join(format!("{name}{extension}"))).filter(|path| path.exists());
        let mounts = base.chain(dlcs).map(|path| mount(&path)).collect::<Result<_, _>>()?;
        Ok(Self::new(mounts))
    }

    pub fn mounts(&self) -> &[Mount] {
        &self.mounts
    }

    /// Where a game path is read from (the latest mount that has it: an archive's path, or a folder's), so a
    /// file that can't be understood names the mod it came from.
    pub fn source(&self, path: &str) -> Option<&Path> {
        self.mount_index(path).map(|index| self.mounts[index].name())
    }

    /// Of `paths`, the mount the highest of them is read from (the latest mount that has any of them): the one that
    /// overrides the others' (a sector's `.base` from a mod over the game's `.data`: the mod broke the pair).
    pub fn topmost_source(&self, paths: &[&str]) -> Option<&Path> {
        paths.iter().filter_map(|path| self.mount_index(path)).max().map(|index| self.mounts[index].name())
    }

    /// The place among the mounts of the latest that has `path`.
    fn mount_index(&self, path: &str) -> Option<usize> {
        self.mounts.iter().rposition(|mount| match mount {
            Mount::Archive(archive) => archive.contains(path),
            Mount::Folder(root) => root.join(path.trim_start_matches('/')).is_file(),
        })
    }

    /// Mounts `mount` over everything mounted so far (a mod: its files replace the same paths below it).
    pub fn mount(&mut self, mount: Mount) {
        self.mounts.push(mount);
    }

    /// Reads a game path such as `/def/world/road_look.sii` (leading slash optional): from the latest
    /// mount that has it, `None` if none does.
    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>, FileError> {
        for mount in self.mounts.iter().rev() {
            let read = match mount {
                Mount::Archive(archive) => archive.read(path),
                Mount::Folder(root) => match std::fs::read(root.join(path.trim_start_matches('/'))) {
                    Ok(bytes) => Ok(Some(bytes)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    // A folder at the path: no file there.
                    Err(_) if root.join(path.trim_start_matches('/')).is_dir() => Ok(None),
                    Err(error) => Err(ArchiveError::Io(error)),
                },
            };
            match read {
                Ok(Some(bytes)) => return Ok(Some(bytes)),
                Ok(None) => {}
                Err(error) => return Err(file_error(path, mount, error)),
            }
        }
        Ok(None)
    }

    /// A texture's top level as stored ([`crate::archive::Archive::read_texture_top`]), from the latest archive
    /// that has it (not from extracted folders: there, the `.dds` beside the `.tobj` is the picture).
    pub fn read_texture_top(&self, path: &str) -> Result<Option<crate::archive::RawTexture>, FileError> {
        for mount in self.mounts.iter().rev() {
            if let Mount::Archive(archive) = mount
                && archive.contains(path)
            {
                return archive.read_texture_top(path).map_err(|error| file_error(path, mount, error));
            }
        }
        Ok(None)
    }

    /// Reads a texture (a `.tobj` path) from the latest mount that has it: from an archive's texture entry,
    /// or in an extracted folder from the `.dds` file beside its `.tobj`.
    pub fn read_texture(&self, path: &str) -> Result<Option<Texture>, FileError> {
        for mount in self.mounts.iter().rev() {
            let read = match mount {
                Mount::Archive(archive) if archive.contains(path) => archive.read_texture(path),
                Mount::Archive(_) => Ok(None),
                Mount::Folder(root) => {
                    let dds = root.join(path.trim_start_matches('/')).with_extension("dds");
                    match std::fs::read(&dds) {
                        Ok(bytes) => Texture::from_dds(&bytes)
                            .map(Some)
                            .map_err(|what| ArchiveError::Unsupported { hash: 0, what }),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                        Err(error) => Err(ArchiveError::Io(error)),
                    }
                }
            };
            match read {
                Ok(Some(texture)) => return Ok(Some(texture)),
                Ok(None) => {}
                Err(error) => return Err(file_error(path, mount, error)),
            }
        }
        Ok(None)
    }

    /// The names of the files directly in a game directory, across all mounts, sorted.
    pub fn list(&self, dir: &str) -> Result<Vec<String>, FileError> {
        self.names(dir, false)
    }

    /// The names of the directories directly in a game directory, across all mounts, sorted.
    pub fn dirs(&self, dir: &str) -> Result<Vec<String>, FileError> {
        self.names(dir, true)
    }

    fn names(&self, dir: &str, dirs: bool) -> Result<Vec<String>, FileError> {
        let mut names = BTreeSet::new();
        for mount in &self.mounts {
            match mount {
                Mount::Archive(archive) => {
                    let listing = archive.list(dir).map_err(|error| file_error(dir, mount, error))?;
                    names.extend(
                        listing.into_iter().flat_map(|listing| if dirs { listing.dirs } else { listing.files }),
                    );
                }
                Mount::Folder(root) => {
                    let Ok(entries) = std::fs::read_dir(root.join(dir.trim_start_matches('/'))) else { continue };
                    for entry in entries.flatten() {
                        if entry.file_type().is_ok_and(|kind| if dirs { kind.is_dir() } else { kind.is_file() }) {
                            names.insert(entry.file_name().to_string_lossy().into_owned());
                        }
                    }
                }
            }
        }
        Ok(names.into_iter().collect())
    }
}

fn file_error(path: &str, mount: &Mount, error: ArchiveError) -> FileError {
    FileError { path: path.to_owned(), mount: mount.name().to_owned(), error }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_dir::TempDir;

    #[test]
    fn names_the_mount_a_path_is_read_from() {
        let (game, a_mod) = (TempDir::new(), TempDir::new());
        for (root, file) in
            [(&game.path, "map/europe/sec+0011+0001.base"), (&a_mod.path, "map/europe/sec+0011+0001.base")]
        {
            std::fs::create_dir_all(root.join("map/europe")).unwrap();
            std::fs::write(root.join(file), b"sector").unwrap();
        }
        std::fs::write(game.path.join("map/europe/sec+0000+0000.base"), b"sector").unwrap();
        let files = GameFiles::new(vec![Mount::Folder(game.path.clone()), Mount::Folder(a_mod.path.clone())]);
        // The mod's own sector comes from the mod; the one it leaves alone, from the game.
        assert_eq!(files.source("/map/europe/sec+0011+0001.base"), Some(a_mod.path.as_path()));
        assert_eq!(files.source("map/europe/sec+0000+0000.base"), Some(game.path.as_path()));
        assert_eq!(files.source("map/europe/sec+0099+0099.base"), None);
    }

    #[test]
    fn names_the_mount_that_overrides_the_others_of_a_pair() {
        // The game has a sector's .base and .data; a mod brings its own .base alone (the review of 2026-11-11, C4c:
        // the pair's error was blamed on the .data's mount, the game's, so no mod was left out and the map failed).
        let (game, a_mod) = (TempDir::new(), TempDir::new());
        for root in [&game.path, &a_mod.path] {
            std::fs::create_dir_all(root.join("map/europe")).unwrap();
        }
        for file in ["map/europe/sec+0001+0001.base", "map/europe/sec+0001+0001.data"] {
            std::fs::write(game.path.join(file), b"game").unwrap();
        }
        std::fs::write(a_mod.path.join("map/europe/sec+0001+0001.base"), b"mod").unwrap();
        let files = GameFiles::new(vec![Mount::Folder(game.path.clone()), Mount::Folder(a_mod.path.clone())]);
        let pair = ["map/europe/sec+0001+0001.base", "map/europe/sec+0001+0001.data"];
        assert_eq!(files.source(pair[1]), Some(game.path.as_path()), "the .data alone: the game's");
        assert_eq!(files.topmost_source(&pair), Some(a_mod.path.as_path()), "the pair: the mod's, over it");
        // The other way round, and both the game's.
        std::fs::remove_file(a_mod.path.join(pair[0])).unwrap();
        std::fs::write(a_mod.path.join(pair[1]), b"mod").unwrap();
        assert_eq!(files.topmost_source(&pair), Some(a_mod.path.as_path()));
        std::fs::remove_file(a_mod.path.join(pair[1])).unwrap();
        assert_eq!(files.topmost_source(&pair), Some(game.path.as_path()));
        assert_eq!(files.topmost_source(&["map/europe/none.base"]), None);
    }
}
