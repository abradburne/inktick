//! A unit's files in its saves folder: the one DSlot gives (`--saves`), else the system's
//! (`<saves root>/GW/<set>/`, as DSlot lays them out), else, where there is no saves root (a
//! computer), inktick's own data folder.
//!
//! | File | |
//! |---|---|
//! | `<set>.state1` .. `.state6` | Save states, each with its picture. |
//! | `<set>.auto.state` | The resume point: written on leaving, read on starting. |
//!
//! The units have no battery-backed memory: their RAM, high scores and clock included, is kept
//! by the batteries in the unit. The resume point is that memory here.
//!
//! The folder is the game's, shared by every emulator for its system (DSlot's
//! docs/emulators.md): inktick writes only its own files, under its own names and format, so
//! another emulator's files beside them are left alone.
//!
//! Files are written by a thread of their own, so a slow card never holds up a frame, and
//! atomically (a temporary file, synced, then renamed over the old one), so a crash or a flat
//! battery leaves the previous save rather than half of a new one.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;

/// The number of save state slots.
pub const SLOTS: usize = 6;

/// Where a unit's files are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveFiles {
    pub dir: PathBuf,
    pub stem: String,
}

impl SaveFiles {
    pub fn new(dir: PathBuf, stem: &str) -> SaveFiles {
        SaveFiles {
            dir,
            stem: stem.to_owned(),
        }
    }

    fn file(&self, ext: &str) -> PathBuf {
        self.dir.join(format!("{}.{ext}", self.stem))
    }

    /// Slot 0 is the resume point, written on leaving.
    pub fn state(&self, slot: usize) -> PathBuf {
        if slot == 0 {
            self.file("auto.state")
        } else {
            self.file(&format!("state{slot}"))
        }
    }
}

/// The saves folder for a unit, when none is given.
pub fn default_dir(console: &str, stem: &str) -> PathBuf {
    let root = nibkit::paths::saves_root();
    let root = if root.is_dir() {
        root
    } else {
        nibkit::paths::AppDirs::new("inktick").data.join("saves")
    };
    nibkit::paths::saves_dir(&root, console, stem)
}

/// A save state as kept in a slot: the machine, its picture and when it was made.
///
/// ```text
/// "ITKS"  u8 version (1)  u64 made (Unix seconds)  u64 played (seconds)
/// u32 picture length, PNG (the screens, each 256 × 192, stacked)
/// the rest: the machine (inktick_core's save state)
/// ```
///
/// The header is inkdot's, under inktick's own name, so DSlot reads both the same way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub made: u64,
    pub played: u64,
    pub png: Vec<u8>,
    pub machine: Vec<u8>,
}

impl Slot {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(25 + self.png.len() + self.machine.len());
        out.extend_from_slice(b"ITKS\x01");
        out.extend_from_slice(&self.made.to_le_bytes());
        out.extend_from_slice(&self.played.to_le_bytes());
        out.extend_from_slice(&(self.png.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.png);
        out.extend_from_slice(&self.machine);
        out
    }

    pub fn decode(data: &[u8]) -> Option<Slot> {
        if data.len() < 25 || &data[..5] != b"ITKS\x01" {
            return None;
        }
        let u64_at = |i: usize| u64::from_le_bytes(data[i..i + 8].try_into().unwrap_or([0; 8]));
        let png_len = u32::from_le_bytes(data[21..25].try_into().ok()?) as usize;
        let png = data.get(25..25 + png_len)?.to_vec();
        let machine = data.get(25 + png_len..)?.to_vec();
        Some(Slot {
            made: u64_at(5),
            played: u64_at(13),
            png,
            machine,
        })
    }

    /// Reads a slot's file; `None` when there is none or it isn't one.
    pub fn read(path: &Path) -> Option<Slot> {
        Slot::decode(&std::fs::read(path).ok()?)
    }
}

/// Writes `data` to `path` so that the file is always either the old one or all of the new.
pub fn write_atomically(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(data)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)
}

/// The thread that writes files.
pub struct Writer {
    jobs: Option<Sender<(PathBuf, Vec<u8>)>>,
    thread: Option<JoinHandle<()>>,
}

impl Writer {
    pub fn start() -> Writer {
        let (tx, rx) = channel::<(PathBuf, Vec<u8>)>();
        let thread = std::thread::Builder::new()
            .name("inktick-saves".into())
            .spawn(move || {
                for (path, data) in rx {
                    if let Err(e) = write_atomically(&path, &data) {
                        eprintln!("inktick: cannot save {}: {e}", path.display());
                    }
                }
            })
            .expect("start the saves thread");
        Writer {
            jobs: Some(tx),
            thread: Some(thread),
        }
    }

    /// Queues a file to be written.
    pub fn write(&self, path: PathBuf, data: Vec<u8>) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send((path, data));
        }
    }

    /// Waits for every queued file to be written.
    pub fn finish(&mut self) {
        self.jobs = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.finish();
    }
}

/// The time now, in Unix seconds.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_round_trip_and_others_are_refused() {
        let s = Slot {
            made: 1_760_000_000,
            played: 42,
            png: vec![1, 2, 3],
            machine: vec![9; 100],
        };
        assert_eq!(Slot::decode(&s.encode()), Some(s.clone()));
        let mut other = s.encode();
        other[..4].copy_from_slice(b"IKSL");
        assert_eq!(Slot::decode(&other), None, "inkdot's states aren't ours");
    }

    #[test]
    fn written_whole() {
        let dir = std::env::temp_dir().join(format!("inktick-saves-{}", std::process::id()));
        let f = SaveFiles::new(dir.clone(), "gnw_dkong");
        assert_eq!(f.state(0).file_name().unwrap(), "gnw_dkong.auto.state");
        assert_eq!(f.state(3).file_name().unwrap(), "gnw_dkong.state3");
        let mut w = Writer::start();
        w.write(f.state(1), vec![7; 10]);
        w.finish();
        assert_eq!(std::fs::read(f.state(1)).unwrap(), vec![7; 10]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
