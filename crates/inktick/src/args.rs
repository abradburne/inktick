//! The command line: `inktick [OPTIONS] [--saves DIR] [--artwork DIR] [SET]`.
//!
//! Without a set, inktick lists the units it finds and lets the player pick one (as when it is
//! started from the stock OS's Apps). The options (`--view unit`, `--no-resume`, ...) are those
//! in [`crate::options::SPECS`]; DSlot passes the same ones, from its description of inktick.

use crate::options::{Takes, SPECS};
use nibkit::settings::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// What the command line asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct Args {
    /// The unit's set (`gnw_dkong.zip`); `None` to pick one.
    pub set: Option<PathBuf>,
    /// The unit's saves folder (DSlot makes one); else the system's saves folder for it.
    pub saves: Option<PathBuf>,
    /// Folders to look for artwork in, besides `artwork` beside the set.
    pub artwork: Vec<PathBuf>,
    /// Options given, by key (`view`, `resume`, ...).
    pub options: BTreeMap<String, Value>,
}

/// Usage, for `--help` and mistakes.
pub fn usage() -> String {
    let mut s = String::from(
        "usage: inktick [OPTIONS] [--saves DIR] [--artwork DIR] [SET]\n\n\
         SET is a unit's ROM set as MAME has it (gnw_dkong.zip or .7z, with its screen SVGs);\n\
         without one, inktick lists the units it finds and you pick one.\n\
         --saves DIR      where the unit's resume point and save states go (default: the\n\
         \x20                system's saves folder, <saves>/GW/<set>/)\n\
         --artwork DIR    where to look for the unit's artwork (<set>.zip with default.lay),\n\
         \x20                besides the artwork folder beside the set\n\
         --units          list the units inktick plays: set, title, model, year, series\n\
         \x20                (tab-separated), and exit\n\nOptions:\n",
    );
    for o in SPECS {
        let values = match o.takes {
            Takes::Choice(c) => c.iter().map(|(v, _)| *v).collect::<Vec<_>>().join("|"),
            Takes::Toggle => format!("--{0} / --no-{0}", o.key),
            Takes::Number { min, max, .. } => format!("{min}..{max}"),
        };
        s += &format!("  --{:<9} {:<26} {values}\n", o.key, o.label);
    }
    s
}

/// Reads the arguments (without the program's name).
pub fn parse(words: &[String]) -> Result<Args, String> {
    let mut args = Args {
        set: None,
        saves: None,
        artwork: Vec::new(),
        options: BTreeMap::new(),
    };
    let mut i = 0;
    while i < words.len() {
        let rest = &words[i..];
        if let Some((key, result)) = SPECS
            .iter()
            .find_map(|o| o.recognise(rest).map(|r| (o.key, r)))
        {
            let (n, value) = result?;
            args.options.insert(key.to_owned(), value);
            i += n;
            continue;
        }
        let value = || {
            words
                .get(i + 1)
                .cloned()
                .ok_or(format!("{} needs a value", words[i]))
        };
        match words[i].as_str() {
            "--saves" => args.saves = Some(PathBuf::from(value()?)),
            "--artwork" => args.artwork.push(PathBuf::from(value()?)),
            w if w.starts_with("--") => return Err(format!("unknown option {w}")),
            w => {
                if args.set.replace(PathBuf::from(w)).is_some() {
                    return Err("more than one set".into());
                }
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn a_full_command_line() {
        let a = parse(&words(
            "--view unit --no-resume --saves /s/GW/gnw_dkong --artwork /a /r/gnw_dkong.zip",
        ))
        .unwrap();
        assert_eq!(a.set, Some(PathBuf::from("/r/gnw_dkong.zip")));
        assert_eq!(a.saves, Some(PathBuf::from("/s/GW/gnw_dkong")));
        assert_eq!(a.artwork, vec![PathBuf::from("/a")]);
        assert_eq!(a.options["view"], Value::from("unit"));
        assert_eq!(a.options["resume"], Value::Boolean(false));
    }

    #[test]
    fn mistakes_are_refused() {
        assert!(parse(&words("--view sepia a.zip")).is_err());
        assert!(parse(&words("a.zip b.zip")).is_err());
        assert!(parse(&words("--saves")).is_err());
        assert!(parse(&words("--sepia a.zip")).is_err());
        assert_eq!(parse(&[]).unwrap().set, None);
    }

    #[test]
    fn every_option_is_in_the_usage() {
        let u = usage();
        for o in SPECS {
            assert!(u.contains(&format!("--{}", o.key)), "{}", o.key);
        }
    }

    /// The description DSlot reads declares exactly inktick's options, with the same values,
    /// defaults and command-line forms.
    #[cfg(feature = "dslot")]
    #[test]
    fn the_dslot_description_matches() {
        let text = include_str!("../../../packaging/nibos/inktick.toml");
        let emulator = nibkit::dslot::check("inktick", text).expect("a valid description");
        assert_eq!(emulator.options.len(), SPECS.len());
        let opts = crate::options::Options::new(
            "GW",
            "x",
            BTreeMap::new(),
            nibkit::settings::Settings::load(std::env::temp_dir().join("inktick-desc.toml")),
        );
        for (o, spec) in emulator.options.iter().zip(SPECS) {
            assert_eq!(o.def, spec.def(), "{}", spec.key);
            for (value, _) in opts.values(spec.key) {
                let w = o.words(&value);
                assert_eq!(
                    spec.recognise(&w),
                    Some(Ok((w.len(), value.clone()))),
                    "{} {value}",
                    spec.key
                );
            }
        }
    }
}
