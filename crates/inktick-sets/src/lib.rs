//! A unit's ROM set, as MAME distributes it: an archive named for the unit holding its program
//! ROM, its melody ROM (SM511/SM512) and one SVG per screen; or a clone's archive holding only
//! its screens, with the ROM in its parent's; and optionally an artwork archive of the same
//! name with a layout file and pictures.

pub mod archive;

use archive::Archive;
use inktick_art::layout::Layout;
use inktick_art::svg::{self, Screen};
use inktick_core::Kind;
use std::path::{Path, PathBuf};

/// A unit, read from its set.
pub struct Set {
    /// The set's name (the archive's, without extension): `gnw_dkong`.
    pub name: String,
    /// The program ROM, the whole 4,096-byte address space.
    pub program: Box<[u8; 4096]>,
    /// The melody ROM, for SM511/SM512 units.
    pub melody: Option<Box<[u8; 256]>>,
    /// The chip, from the ROMs and screens.
    pub kind: Kind,
    /// The screens, in MAME's order (top before bottom, left before right), with their files.
    pub screens: Vec<(String, Screen)>,
    /// The artwork, if found.
    pub artwork: Option<Artwork>,
    /// The archive the ROM came from.
    pub rom_from: PathBuf,
}

/// An artwork archive: its layout and pictures.
pub struct Artwork {
    /// The layout file.
    pub layout: Layout,
    /// The archive, for the pictures.
    pub files: Archive,
    /// Where it was found.
    pub path: PathBuf,
}

/// The set's name from its file: `…/gnw_dkong.7z` is `gnw_dkong`.
pub fn set_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

fn read(path: &Path) -> Result<Archive, String> {
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Archive::read(&data).map_err(|e| format!("{}: {e}", path.display()))
}

/// The screens an archive holds for `name`, in order.
fn screens_for(a: &Archive, name: &str) -> Result<Vec<(String, Screen)>, String> {
    let order = ["", "_top", "_bottom", "_left", "_right"];
    let mut out = Vec::new();
    for suffix in order {
        let file = format!("{name}{suffix}.svg");
        if let Some(data) = a.get(&file) {
            let text = String::from_utf8_lossy(data);
            out.push((
                file.clone(),
                svg::parse(&text).map_err(|e| format!("{file}: {e}"))?,
            ));
        }
    }
    Ok(out)
}

/// A program ROM and, for SM511/SM512 units, a melody ROM.
type Roms = (Box<[u8; 4096]>, Option<Box<[u8; 256]>>);

/// The ROMs in an archive: the program (a 4,096-byte file, `.program` if named so) and the
/// melody (`.melody`, 256 bytes). With several programs (a parent holding its clones'), the one
/// named `prefer` if given.
fn roms(a: &Archive, prefer: Option<&str>) -> Option<Roms> {
    let programs: Vec<(&String, &Vec<u8>)> = a
        .files
        .iter()
        .filter(|(k, v)| v.len() == 4096 && !k.ends_with(".svg"))
        .collect();
    let (pname, program) = match prefer.and_then(|p| programs.iter().find(|(k, _)| k.as_str() == p))
    {
        Some(p) => *p,
        None => *programs.first()?,
    };
    let stem = pname.strip_suffix(".program").unwrap_or(pname);
    let melody = a
        .files
        .iter()
        .find(|(k, v)| v.len() == 256 && k.as_str() == format!("{stem}.melody"))
        .or_else(|| {
            a.files
                .iter()
                .find(|(k, v)| v.len() == 256 && k.ends_with(".melody"))
        })
        .map(|(_, v)| Box::new(<[u8; 256]>::try_from(v.as_slice()).unwrap_or([0; 256])));
    Some((
        Box::new(<[u8; 4096]>::try_from(program.as_slice()).ok()?),
        melody,
    ))
}

/// Opens the set at `path`. Its ROM is taken from its own archive, else from `parent` (a set
/// name in the same folder), else from any archive in the folder that holds this set's screens
/// beside a ROM (MAME's merged sets). Artwork is looked for in `artwork_dirs`, by the set's
/// name, then its parent's.
pub fn open(path: &Path, parent: Option<&str>, artwork_dirs: &[PathBuf]) -> Result<Set, String> {
    let name = set_name(path);
    let own = read(path)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut screens = screens_for(&own, &name)?;
    let mut found = roms(&own, None).map(|r| (r, path.to_path_buf()));
    let siblings = || -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p != path && matches!(p.extension().and_then(|e| e.to_str()), Some("zip" | "7z"))
            })
            .collect();
        v.sort();
        v
    };
    if found.is_none() {
        let mut candidates = Vec::new();
        if let Some(p) = parent {
            for ext in ["7z", "zip"] {
                candidates.push(dir.join(format!("{p}.{ext}")));
            }
        }
        candidates.extend(siblings());
        for c in candidates {
            let Ok(a) = read(&c) else { continue };
            let named_parent = parent.is_some_and(|p| set_name(&c) == p);
            if !named_parent && screens_for(&a, &name).map_or(true, |s| s.is_empty()) {
                continue;
            }
            if let Some(r) = roms(&a, None) {
                if screens.is_empty() {
                    screens = screens_for(&a, &name)?;
                }
                found = Some((r, c));
                break;
            }
        }
    }
    let ((program, melody), rom_from) = found.ok_or_else(|| {
        if own.files.values().any(|v| v.len() == 1856) {
            format!("{name}: an SM5A unit, not supported yet")
        } else {
            format!("{name}: no program ROM in the set or its parent")
        }
    })?;
    if screens.is_empty() {
        return Err(format!("{name}: no screen SVG"));
    }
    let c_segments = screens
        .iter()
        .any(|(_, s)| s.segments.iter().any(|g| g.addr[0] == 3));
    let kind = match (&melody, c_segments) {
        (Some(_), true) => Kind::Sm512,
        (Some(_), false) => Kind::Sm511,
        (None, _) => Kind::Sm510,
    };
    let mut artwork = None;
    'look: for d in artwork_dirs {
        for n in [Some(name.as_str()), parent].into_iter().flatten() {
            for ext in ["zip", "7z"] {
                let p = d.join(format!("{n}.{ext}"));
                if p.is_file() {
                    let files = read(&p)?;
                    let lay = files
                        .get("default.lay")
                        .ok_or(format!("{}: no default.lay", p.display()))?;
                    let layout = Layout::parse(&String::from_utf8_lossy(lay))
                        .map_err(|e| format!("{}: {e}", p.display()))?;
                    artwork = Some(Artwork {
                        layout,
                        files,
                        path: p,
                    });
                    break 'look;
                }
            }
        }
    }
    Ok(Set {
        name,
        program,
        melody,
        kind,
        screens,
        artwork,
        rom_from,
    })
}

impl Artwork {
    /// A picture from the archive.
    pub fn picture(&self, file: &str) -> Option<inktick_art::Image> {
        inktick_art::Image::from_png(self.files.get(file)?).ok()
    }
}
