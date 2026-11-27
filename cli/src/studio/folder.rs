//! A folder that the studio's picker opens once, from the browse root or the
//! clone workspace down, without following a symbolic link at any step. The
//! picker then lists, reads and creates files relative to that open folder,
//! so a folder that someone swaps for a link after acu's check cannot send
//! those calls elsewhere (issue #45). On Unix the folder holds a file
//! descriptor, and the calls go through rustix (`openat` and its kin). On
//! other systems the calls take paths.

use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

#[cfg(unix)]
use rustix::fs::{AtFlags, FileType, Mode, OFlags};

/// The most bytes that [`Folder::read_small`] reads from one file: 1 MB.
const SMALL: u64 = 1 << 20;

/// How acu opens a folder: to read, never through a link, closed in child
/// programs.
#[cfg(unix)]
const OPEN_DIR: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

/// A folder that acu opened once. Each name that a method takes names one
/// entry directly inside the folder, never a path.
pub struct Folder {
    /// The folder's real path when acu opened it, for messages and for the
    /// programs that take a path, such as the adapters.
    path: PathBuf,
    /// The open folder.
    #[cfg(unix)]
    fd: std::os::fd::OwnedFd,
}

/// Returns a name of one entry, or refuses a name that holds a separator or
/// names the folder itself or its parent.
fn plain(name: &str) -> io::Result<&str> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name:?} names no single entry of a folder"),
        ));
    }
    Ok(name)
}

/// Creates a new file at `name` in the open folder `dir`, writes the text
/// and syncs it. It never opens a file that exists, nor a link.
#[cfg(unix)]
fn write_new(dir: &std::os::fd::OwnedFd, name: &str, text: &str) -> io::Result<()> {
    let flags = OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mode = Mode::RUSR | Mode::WUSR | Mode::RGRP | Mode::WGRP | Mode::ROTH | Mode::WOTH;
    let mut f = std::fs::File::from(rustix::fs::openat(dir, name, flags, mode)?);
    f.write_all(text.as_bytes())?;
    f.sync_all()
}

impl Folder {
    /// Opens `path`, a real path inside the real path `base`, as
    /// `canonicalize` gives them. acu opens `base` by its path, then each
    /// name below it without following a link, so a link at any step fails
    /// the open.
    pub fn open(base: &Path, path: &Path) -> io::Result<Folder> {
        let rel = path.strip_prefix(base).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} lies outside {}", path.display(), base.display()),
            )
        })?;
        let names = rel.components().map(|c| match c {
            Component::Normal(n) => Ok(n),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} holds a step that is no plain name", rel.display()),
            )),
        });
        #[cfg(unix)]
        {
            let mut fd = rustix::fs::openat(
                rustix::fs::CWD,
                base,
                OPEN_DIR.difference(OFlags::NOFOLLOW),
                Mode::empty(),
            )?;
            for name in names {
                fd = rustix::fs::openat(&fd, name?, OPEN_DIR, Mode::empty())?;
            }
            Ok(Folder {
                path: path.to_path_buf(),
                fd,
            })
        }
        #[cfg(not(unix))]
        {
            let mut at = base.to_path_buf();
            for name in names {
                at.push(name?);
                if !std::fs::symlink_metadata(&at)?.is_dir() {
                    return Err(io::Error::other(format!("{} is no folder", at.display())));
                }
            }
            Ok(Folder {
                path: path.to_path_buf(),
            })
        }
    }

    /// Returns the folder's path as acu opened it.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Opens the folder `name` inside this one, never through a link. Gives
    /// `None` for anything else.
    pub fn sub(&self, name: &str) -> Option<Folder> {
        let name = plain(name).ok()?;
        let path = self.path.join(name);
        #[cfg(unix)]
        {
            let fd = rustix::fs::openat(&self.fd, name, OPEN_DIR, Mode::empty()).ok()?;
            Some(Folder { path, fd })
        }
        #[cfg(not(unix))]
        {
            std::fs::symlink_metadata(&path)
                .ok()?
                .is_dir()
                .then_some(Folder { path })
        }
    }

    /// Lists entry names in the order that the system gives them, up to
    /// `limit` names that `keep` accepts. With `dirs`, only folders count,
    /// never a link to one.
    pub fn names(
        &self,
        dirs: bool,
        keep: impl Fn(&str) -> bool,
        limit: usize,
    ) -> io::Result<Vec<String>> {
        let mut out = Vec::new();
        #[cfg(unix)]
        for e in rustix::fs::Dir::read_from(&self.fd)? {
            if out.len() >= limit {
                break;
            }
            let Ok(e) = e else { continue };
            let name = e.file_name().to_string_lossy().into_owned();
            if name == "." || name == ".." || !keep(&name) {
                continue;
            }
            if dirs {
                // Some file systems leave the type out of a listing.
                let kind = match e.file_type() {
                    FileType::Unknown => {
                        rustix::fs::statat(&self.fd, e.file_name(), AtFlags::SYMLINK_NOFOLLOW)
                            .map(|s| FileType::from_raw_mode(s.st_mode))
                            .unwrap_or(FileType::Unknown)
                    }
                    t => t,
                };
                if kind != FileType::Directory {
                    continue;
                }
            }
            out.push(name);
        }
        #[cfg(not(unix))]
        for e in std::fs::read_dir(&self.path)? {
            if out.len() >= limit {
                break;
            }
            let Ok(e) = e else { continue };
            let name = e.file_name().to_string_lossy().into_owned();
            if !keep(&name) || (dirs && !e.file_type().is_ok_and(|t| t.is_dir())) {
                continue;
            }
            out.push(name);
        }
        Ok(out)
    }

    /// Reports whether `name` leads to a plain file, through a link too, as
    /// [`Path::is_file`] does.
    pub fn is_file(&self, name: &str) -> bool {
        let Ok(name) = plain(name) else { return false };
        #[cfg(unix)]
        {
            rustix::fs::statat(&self.fd, name, AtFlags::empty())
                .is_ok_and(|s| FileType::from_raw_mode(s.st_mode) == FileType::RegularFile)
        }
        #[cfg(not(unix))]
        {
            self.path.join(name).is_file()
        }
    }

    /// Reports whether `name` leads to anything, through a link too, as
    /// [`Path::exists`] does.
    pub fn exists(&self, name: &str) -> bool {
        let Ok(name) = plain(name) else { return false };
        #[cfg(unix)]
        {
            rustix::fs::statat(&self.fd, name, AtFlags::empty()).is_ok()
        }
        #[cfg(not(unix))]
        {
            self.path.join(name).exists()
        }
    }

    /// Reports whether `name` is a symbolic link.
    pub fn is_link(&self, name: &str) -> bool {
        let Ok(name) = plain(name) else { return false };
        #[cfg(unix)]
        {
            rustix::fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW)
                .is_ok_and(|s| FileType::from_raw_mode(s.st_mode) == FileType::Symlink)
        }
        #[cfg(not(unix))]
        {
            std::fs::symlink_metadata(self.path.join(name))
                .is_ok_and(|m| m.file_type().is_symlink())
        }
    }

    /// Reads the small text file `name`, or returns nothing. It reads a plain
    /// file only, never through a link, and at most 1 MB whatever the file
    /// claims: a repository could point a marker file at /dev/zero or at a
    /// file outside it.
    pub fn read_small(&self, name: &str) -> String {
        let mut out = String::new();
        let Ok(name) = plain(name) else { return out };
        #[cfg(unix)]
        {
            // Without blocking, so a pipe at the name cannot stall the open.
            let flags = OFlags::RDONLY
                | OFlags::NOFOLLOW
                | OFlags::CLOEXEC
                | OFlags::NONBLOCK
                | OFlags::NOCTTY;
            let Ok(fd) = rustix::fs::openat(&self.fd, name, flags, Mode::empty()) else {
                return out;
            };
            let file = rustix::fs::fstat(&fd)
                .is_ok_and(|s| FileType::from_raw_mode(s.st_mode) == FileType::RegularFile);
            if file {
                let _ = std::fs::File::from(fd).take(SMALL).read_to_string(&mut out);
            }
        }
        #[cfg(not(unix))]
        {
            let p = self.path.join(name);
            if std::fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_file()) {
                if let Ok(f) = std::fs::File::open(&p) {
                    let _ = f.take(SMALL).read_to_string(&mut out);
                }
            }
        }
        out
    }

    /// Writes a new file `name` in one step and never replaces one: the text
    /// goes to a temporary file first, which then gets the final name through
    /// a hard link, which fails when that name exists. A crash leaves no half
    /// file.
    pub fn create_new(&self, name: &str, text: &str) -> io::Result<()> {
        let name = plain(name)?;
        let tmp = format!(".{name}.acu-new-{}", std::process::id());
        #[cfg(unix)]
        {
            let done = (|| -> io::Result<()> {
                write_new(&self.fd, &tmp, text)?;
                match rustix::fs::linkat(&self.fd, tmp.as_str(), &self.fd, name, AtFlags::empty()) {
                    Err(e) if e == rustix::io::Errno::EXIST => return Err(e.into()),
                    // A file system without hard links (exFAT, some network
                    // shares): write the final name directly, still never
                    // over a file.
                    Err(_) => write_new(&self.fd, name, text)?,
                    Ok(()) => {}
                }
                // Make the new name itself durable.
                rustix::fs::fsync(&self.fd)?;
                Ok(())
            })();
            let _ = rustix::fs::unlinkat(&self.fd, tmp.as_str(), AtFlags::empty());
            done
        }
        #[cfg(not(unix))]
        {
            let (tmp, file) = (self.path.join(tmp), self.path.join(name));
            let write = |p: &Path| -> io::Result<()> {
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(p)?;
                f.write_all(text.as_bytes())?;
                f.sync_all()
            };
            let done = write(&tmp).and_then(|_| match std::fs::hard_link(&tmp, &file) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(e),
                Err(_) => write(&file),
                Ok(()) => Ok(()),
            });
            let _ = std::fs::remove_file(&tmp);
            done
        }
    }

    /// Checks that the folder's path still leads to this open folder, with
    /// no link on the way, so a program that takes the path reaches the
    /// folder that acu checked. A swap after this check still reaches that
    /// program.
    pub fn still_at_path(&self) -> io::Result<()> {
        let moved = || io::Error::other("another folder or a link took its place");
        if self.path.canonicalize()? != self.path {
            return Err(moved());
        }
        #[cfg(unix)]
        {
            let (open, now) = (rustix::fs::fstat(&self.fd)?, rustix::fs::stat(&self.path)?);
            if (open.st_dev, open.st_ino) != (now.st_dev, now.st_ino) {
                return Err(moved());
            }
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// Makes a base folder with a folder `pick` that holds `go.mod`, and a
    /// folder `outside` beside the base that holds another `go.mod`.
    fn tree() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let base = top.join("base");
        std::fs::create_dir_all(base.join("pick/sub")).unwrap();
        std::fs::write(base.join("pick/go.mod"), "inside\n").unwrap();
        std::fs::create_dir_all(top.join("outside/deep")).unwrap();
        std::fs::write(top.join("outside/go.mod"), "outside\n").unwrap();
        (dir, base)
    }

    #[test]
    fn a_link_on_the_way_fails_the_open() {
        let (_dir, base) = tree();
        symlink(base.join("pick"), base.join("link")).unwrap();
        assert!(Folder::open(&base, &base.join("link/sub")).is_err());
        assert!(Folder::open(&base, &base.join("link")).is_err());
        assert!(Folder::open(&base, &base.parent().unwrap().join("outside")).is_err());
        let f = Folder::open(&base, &base.join("pick/sub")).unwrap();
        assert_eq!(f.path(), base.join("pick/sub"));
        f.still_at_path().unwrap();
    }

    #[test]
    fn calls_after_a_swap_stay_in_the_opened_folder() {
        let (_dir, base) = tree();
        let f = Folder::open(&base, &base.join("pick")).unwrap();
        // Someone swaps the folder for a link to a folder outside the base.
        std::fs::rename(base.join("pick"), base.join("aside")).unwrap();
        symlink(base.parent().unwrap().join("outside"), base.join("pick")).unwrap();
        assert_eq!(f.read_small("go.mod"), "inside\n");
        assert_eq!(f.names(true, |_| true, 10).unwrap(), vec!["sub"]);
        assert!(f.sub("deep").is_none());
        f.create_new("atlas.yaml", "version: 1\n").unwrap();
        assert!(base.join("aside/atlas.yaml").is_file());
        assert!(!base.parent().unwrap().join("outside/atlas.yaml").exists());
        assert!(f.still_at_path().is_err());
    }

    #[test]
    fn names_skip_links_and_take_one_entry_only() {
        let (_dir, base) = tree();
        let f = Folder::open(&base, &base).unwrap();
        symlink(base.join("pick"), base.join("link")).unwrap();
        symlink(base.join("pick/go.mod"), base.join("mod")).unwrap();
        assert_eq!(f.names(true, |_| true, 10).unwrap(), vec!["pick"]);
        let mut all = f.names(false, |_| true, 10).unwrap();
        all.sort();
        assert_eq!(all, vec!["link", "mod", "pick"]);
        assert_eq!(f.names(false, |_| true, 1).unwrap().len(), 1);
        assert!(f.is_link("link") && f.is_link("mod") && !f.is_link("pick"));
        // A test that follows a link, as Path::is_file does; a read never.
        assert!(f.is_file("mod") && f.exists("link"));
        assert_eq!(f.read_small("mod"), "");
        for bad in ["", ".", "..", "pick/go.mod", "a\\b"] {
            assert!(f.sub(bad).is_none() && !f.exists(bad), "{bad:?}");
            assert!(f.create_new(bad, "x").is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_read_stops_at_a_pipe_a_device_and_the_size_limit() {
        let (_dir, base) = tree();
        let f = Folder::open(&base, &base).unwrap();
        let fifo = base.join("fifo");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .is_ok_and(|s| s.success());
        if made {
            assert_eq!(f.read_small("fifo"), "");
        }
        std::fs::write(base.join("big"), "x".repeat((SMALL + 10) as usize)).unwrap();
        assert_eq!(f.read_small("big").len() as u64, SMALL);
        let dev = Folder::open(Path::new("/dev"), Path::new("/dev")).unwrap();
        assert_eq!(dev.read_small("zero"), "");
    }
}
