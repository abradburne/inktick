//! The game options: what they are, what they're set to and where that comes from.
//!
//! inktick defines its options here, by itself: it runs without DSlot (`inktick gnw_dkong.zip`, or
//! from the stock OS's Apps). For DSlot it also ships a description file declaring the same
//! options (`packaging/nibos/inktick.toml`), so DSlot can show them per console and per game and
//! pass them on the command line; a test (feature `dslot`) checks the two agree.
//!
//! A value comes from, first to last: a change made in inktick's own menu during this run, the
//! command line, the settings file (for the game, then for its console), the option's default.
//! The settings file is DSlot's when inktick is built for DSlot and DSlot has one, so its menu and
//! inktick's show the same values; otherwise inktick's own (`~/.config/inktick/settings.toml` on
//! nibOS, beside the program on the stock OS). Either way the keys are `inktick.<option>` at the
//! console level (`[console.GW.inktick]`) and the game level, as DSlot keeps them.

use nibkit::settings::{Def, Kind, Levels, Scope, Settings, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// What values an option takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Takes {
    /// One of these (value, label).
    Choice(&'static [(&'static str, &'static str)]),
    /// On or off: `--key` and `--no-key` on the command line.
    Toggle,
    /// A whole number in a range, in steps, shown with a unit.
    Number {
        min: i64,
        max: i64,
        step: i64,
        unit: &'static str,
    },
}

/// An option's default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Default {
    Str(&'static str),
    Bool(bool),
    Int(i64),
}

/// One option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spec {
    pub key: &'static str,
    pub label: &'static str,
    /// The consoles it is for; empty for both.
    pub consoles: &'static [&'static str],
    pub takes: Takes,
    pub default: Default,
}

/// Every option, in the order the menus show them.
pub const SPECS: &[Spec] = &[
    Spec {
        key: "view",
        label: "View",
        consoles: &[],
        takes: Takes::Choice(&[
            ("lcd", "The screens, large"),
            ("unit", "The whole unit"),
            ("plain", "Plain LCD"),
        ]),
        default: Default::Str("lcd"),
    },
    Spec {
        key: "ghosting",
        label: "LCD response",
        consoles: &[],
        takes: Takes::Choice(&[
            ("unit", "As the unit"),
            ("slow", "Slow"),
            ("off", "Instant"),
        ]),
        default: Default::Str("unit"),
    },
    Spec {
        key: "shadow",
        label: "Segment shadows",
        consoles: &[],
        takes: Takes::Choice(&[("soft", "Soft"), ("deep", "Deep"), ("off", "Off")]),
        default: Default::Str("soft"),
    },
    Spec {
        key: "ink",
        label: "Segment contrast",
        consoles: &[],
        takes: Takes::Number {
            min: 60,
            max: 140,
            step: 10,
            unit: "%",
        },
        default: Default::Int(100),
    },
    Spec {
        key: "sound",
        label: "Sound",
        consoles: &[],
        takes: Takes::Choice(&[("piezo", "Piezo"), ("clean", "Clean")]),
        default: Default::Str("piezo"),
    },
    Spec {
        key: "clock",
        label: "The unit's clock",
        consoles: &[],
        takes: Takes::Choice(&[("system", "Set from the system"), ("own", "Its own")]),
        default: Default::Str("system"),
    },
    Spec {
        key: "bottom",
        label: "Bottom screen in play",
        consoles: &[],
        takes: Takes::Choice(&[
            ("dim", "Dim when idle"),
            ("off", "Off when idle"),
            ("on", "Always lit"),
        ]),
        default: Default::Str("dim"),
    },
    Spec {
        key: "resume",
        label: "Resume where I left off",
        consoles: &[],
        takes: Takes::Toggle,
        default: Default::Bool(true),
    },
];

impl Spec {
    pub fn applies(&self, console: &str) -> bool {
        self.consoles.is_empty() || self.consoles.contains(&console)
    }

    pub fn default_value(&self) -> Value {
        match self.default {
            Default::Str(s) => Value::from(s),
            Default::Bool(b) => Value::Boolean(b),
            Default::Int(n) => Value::Integer(n),
        }
    }

    /// The option as a setting, keyed as DSlot keys an emulator's options.
    pub fn def(&self) -> Def {
        let kind = match self.takes {
            Takes::Choice(c) => Kind::choice(c),
            Takes::Toggle => Kind::Toggle,
            Takes::Number {
                min,
                max,
                step,
                unit,
            } => Kind::Number {
                min,
                max,
                step,
                unit: unit.into(),
            },
        };
        Def {
            key: format!("inktick.{}", self.key),
            label: self.label.into(),
            kind,
            levels: Levels::CONSOLE_AND_GAME,
            default: Some(self.default_value()),
        }
    }

    /// Reads the option from the command line at `words`: how many words it took and its value.
    pub fn recognise(&self, words: &[String]) -> Option<Result<(usize, Value), String>> {
        let flag = format!("--{}", self.key);
        let first = words.first()?;
        if self.takes == Takes::Toggle {
            if *first == flag {
                return Some(Ok((1, Value::Boolean(true))));
            }
            return (*first == format!("--no-{}", self.key))
                .then_some(Ok((1, Value::Boolean(false))));
        }
        if *first != flag {
            return None;
        }
        let Some(word) = words.get(1) else {
            return Some(Err(format!("{flag} needs a value")));
        };
        let value = word
            .parse::<i64>()
            .map(Value::Integer)
            .unwrap_or_else(|_| Value::from(word.as_str()));
        if self.def().accepts(&value) {
            Some(Ok((2, value)))
        } else {
            Some(Err(format!("{flag} {word}: not one of its values")))
        }
    }
}

/// Where a value came from, as the Options page says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Launch,
    Game,
    Console,
    Default,
}

/// The settings file. Built for DSlot, where DSlot has one (on nibOS), DSlot's: each option DSlot
/// shows lives there and nowhere else (DSlot's docs/emulators.md, "One place for each option"),
/// so inktick started by hand sees the same choices. Else inktick's own.
pub fn settings_file() -> PathBuf {
    #[cfg(feature = "dslot")]
    if let Some(file) = nibkit::dslot::settings_file() {
        return file;
    }
    nibkit::paths::AppDirs::new("inktick")
        .config
        .join("settings.toml")
}

/// The options for one game.
pub struct Options {
    console: String,
    rom_name: String,
    given: BTreeMap<String, Value>,
    /// Options changed in the menu this run: what the command line gave no longer stands.
    touched: std::collections::BTreeSet<String>,
    pub settings: Settings,
}

impl Options {
    pub fn new(
        console: &str,
        rom_name: &str,
        given: BTreeMap<String, Value>,
        settings: Settings,
    ) -> Options {
        Options {
            console: console.to_owned(),
            rom_name: rom_name.to_owned(),
            given,
            touched: std::collections::BTreeSet::new(),
            settings,
        }
    }

    /// The keys of the options for this game's console, in order.
    pub fn keys(&self) -> Vec<&'static str> {
        SPECS
            .iter()
            .filter(|s| s.applies(&self.console))
            .map(|s| s.key)
            .collect()
    }

    fn spec(key: &str) -> Option<&'static Spec> {
        SPECS.iter().find(|s| s.key == key)
    }

    fn full(key: &str) -> String {
        format!("inktick.{key}")
    }

    /// An option's value and where it came from.
    pub fn get(&self, key: &str) -> (Value, Source) {
        let Some(spec) = Self::spec(key) else {
            return (Value::Boolean(false), Source::Default);
        };
        let def = spec.def();
        let ok = |v: &&Value| def.accepts(v);
        let full = Self::full(key);
        let game = Scope::Game(Scope::game_key(&self.console, &self.rom_name));
        let console = Scope::Console(self.console.clone());
        let kept = if let Some(v) = self.settings.get(&game, &full).filter(ok) {
            (v.clone(), Source::Game)
        } else if let Some(v) = self.settings.get(&console, &full).filter(ok) {
            (v.clone(), Source::Console)
        } else {
            (spec.default_value(), Source::Default)
        };
        // DSlot passes the file's values as flags too: those are still the file's.
        match self
            .given
            .get(key)
            .filter(ok)
            .filter(|_| !self.touched.contains(key))
        {
            Some(v) if *v != kept.0 => (v.clone(), Source::Launch),
            _ => kept,
        }
    }

    pub fn str(&self, key: &str) -> String {
        self.get(key).0.as_str().unwrap_or("").to_owned()
    }

    pub fn bool(&self, key: &str) -> bool {
        self.get(key).0.as_bool().unwrap_or(false)
    }

    pub fn int(&self, key: &str) -> i64 {
        self.get(key).0.as_integer().unwrap_or(0)
    }

    /// Changes an option from the menu, for the game or for its console, and keeps it.
    pub fn set(&mut self, key: &str, value: Value, for_game: bool) {
        let Some(spec) = Self::spec(key) else { return };
        if !spec.def().accepts(&value) {
            return;
        }
        self.settings
            .set(self.scope(for_game), &Self::full(key), value);
        self.touched.insert(key.to_owned());
        self.save();
    }

    /// Clears an option at the game's level or its console's, so the next level's value (or the
    /// default) applies again.
    pub fn unset(&mut self, key: &str, for_game: bool) {
        self.settings.reset(&self.scope(for_game), &Self::full(key));
        self.touched.insert(key.to_owned());
        self.save();
    }

    /// The value set at the game's level or its console's, if any.
    pub fn here(&self, key: &str, for_game: bool) -> Option<Value> {
        let def = Self::spec(key)?.def();
        self.settings
            .get(&self.scope(for_game), &Self::full(key))
            .filter(|v| def.accepts(v))
            .cloned()
    }

    /// What an option is when not set at that level, and where that comes from: the console's
    /// setting (for the game's level), else the default.
    pub fn inherited(&self, key: &str, for_game: bool) -> (Value, String) {
        if for_game {
            if let Some(v) = self.here(key, false) {
                return (v, self.console.clone());
            }
        }
        let default = Self::spec(key).map_or(Value::Boolean(false), Spec::default_value);
        (default, "default".into())
    }

    fn scope(&self, for_game: bool) -> Scope {
        if for_game {
            Scope::Game(Scope::game_key(&self.console, &self.rom_name))
        } else {
            Scope::Console(self.console.clone())
        }
    }

    fn save(&self) {
        if let Err(e) = self.settings.save() {
            eprintln!(
                "inktick: cannot save settings to {}: {e}",
                self.settings.file().display()
            );
        }
    }

    /// An option's values with their labels, for stepping through them.
    pub fn values(&self, key: &str) -> Vec<(Value, String)> {
        match Self::spec(key).map(|s| s.takes) {
            Some(Takes::Choice(c)) => c
                .iter()
                .map(|(v, l)| (Value::from(*v), (*l).to_owned()))
                .collect(),
            Some(Takes::Toggle) => vec![
                (Value::Boolean(false), "Off".into()),
                (Value::Boolean(true), "On".into()),
            ],
            Some(Takes::Number {
                min,
                max,
                step,
                unit,
            }) => (min..=max)
                .step_by(step.max(1) as usize)
                .map(|n| (Value::Integer(n), format!("{n}{unit}")))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Whether stepping through the values goes round (choices) or stops at the ends (numbers).
    pub fn wraps(&self, key: &str) -> bool {
        !matches!(Self::spec(key).map(|s| s.takes), Some(Takes::Number { .. }))
    }

    /// How a value is shown.
    pub fn show(&self, key: &str, value: &Value) -> String {
        Self::spec(key).map_or_else(String::new, |s| s.def().show(value))
    }

    pub fn label(&self, key: &str) -> String {
        Self::spec(key).map_or_else(String::new, |s| s.label.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(name: &str, given: &[(&str, Value)]) -> Options {
        let file = std::env::temp_dir().join(format!(
            "inktick-options-{name}-{}.toml",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&file);
        let given = given
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect();
        Options::new("GW", "gnw_dkong", given, Settings::load(file))
    }

    #[test]
    fn defaults_the_launch_and_the_menu_in_order() {
        let mut o = options("order", &[("resume", Value::Boolean(false))]);
        assert_eq!(o.get("view"), (Value::from("lcd"), Source::Default));
        assert_eq!(o.get("resume"), (Value::Boolean(false), Source::Launch));
        o.set("resume", Value::Boolean(true), true);
        assert_eq!(o.get("resume"), (Value::Boolean(true), Source::Game));
        o.set("view", "sepia".into(), true);
        assert_eq!(o.str("view"), "lcd");
    }

    #[test]
    fn written_as_dslot_reads_them() {
        let mut o = options("dslot-form", &[]);
        o.set("view", "unit".into(), true);
        o.set("ink", Value::Integer(120), false);
        let s = Settings::load(o.settings.file());
        assert_eq!(
            s.get(&Scope::Game("GW/gnw_dkong".into()), "inktick.view"),
            Some(&Value::from("unit"))
        );
        assert_eq!(
            s.get(&Scope::Console("GW".into()), "inktick.ink"),
            Some(&Value::Integer(120))
        );
    }

    #[test]
    fn numbers_step() {
        let o = options("steps", &[]);
        let v: Vec<i64> = o
            .values("ink")
            .into_iter()
            .filter_map(|(v, _)| v.as_integer())
            .collect();
        assert_eq!(v, vec![60, 70, 80, 90, 100, 110, 120, 130, 140]);
        assert!(!o.wraps("ink") && o.wraps("view"));
    }

    #[test]
    fn command_line_forms() {
        let w = |s: &str| s.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
        let view = &SPECS[0];
        assert_eq!(
            view.recognise(&w("--view unit x")),
            Some(Ok((2, Value::from("unit"))))
        );
        assert!(matches!(view.recognise(&w("--view sepia")), Some(Err(_))));
        let resume = SPECS.iter().find(|s| s.key == "resume").unwrap();
        assert_eq!(
            resume.recognise(&w("--no-resume")),
            Some(Ok((1, Value::Boolean(false))))
        );
    }
}
