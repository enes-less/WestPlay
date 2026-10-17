//! The game's file system: archives mounted on top of each other, a later one's files replacing an
//! earlier one's at the same path (the base game first, then its DLCs, then mods). Each is the game's
//! own `.scs` archive ([`crate::archive`]), or a folder extracted from one (for development).

use std::collections::BTreeSet;
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
}

impl GameFiles {
    /// `mounts` in mount order: a file in a later one replaces the same path in an earlier one.
    pub fn new(mounts: Vec<Mount>) -> Self {
        Self { mounts }
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
        let mut names = BTreeSet::new();
        for mount in &self.mounts {
            match mount {
                Mount::Archive(archive) => {
                    let listing = archive.list(dir).map_err(|error| file_error(dir, mount, error))?;
                    names.extend(listing.into_iter().flat_map(|listing| listing.files));
                }
                Mount::Folder(root) => {
                    let Ok(entries) = std::fs::read_dir(root.join(dir.trim_start_matches('/'))) else { continue };
                    for entry in entries.flatten() {
                        if entry.file_type().is_ok_and(|kind| kind.is_file()) {
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
