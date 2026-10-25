# AGENTS.md: working on inktick

inktick is a Game & Watch emulator written from scratch, for any computer and for the Anbernic
RG DS+ on nibOS. Read this first; [CONTRIBUTING.md](CONTRIBUTING.md) has the rest.

## Clean room: the rule above all others

inktick must stay entirely its authors' own, so it can be licensed freely. **Never read, copy or
paraphrase the source of another emulator or emulator component**: MAME (its source tree,
including its drivers, its CPU cores and the layout and input definitions in them), Yokoi
(RetroValou's simulator), gw-libretro and its LCD converters, PolluxPT's port, any FPGA core
(including the documents kept in its repository), forum posts that paste their code. Work from:

- Sharp's data books (the SM510, SM511 and SM512 chapters), the units' printed instructions and
  pictures of the units;
- file-format documentation (APPNOTE, the 7z format description, SVG 1.1, MAME's user
  documentation of its layout files);
- running a locally installed MAME as a black box: its pictures and sound compared, its source
  never opened, its output never published.

If a needed fact can only be found in another emulator's source, stop and ask. Every source used
is listed in [docs/provenance.md](docs/provenance.md); add a row before using a new one. A
commit that implements hardware behaviour names its source in its body. Never put a ROM, a
set or artwork in the repository.

## Layout

| | |
|---|---|
| `crates/inktick-core` | Chips, machine, unit database (`units_table.rs`, one row per unit), synthesiser, save states. `no_std`, no dependencies, no `unsafe`. |
| `crates/inktick-art` | SVG screens, MAME layouts, optics, the composer, the GPU's data (`Scene::lcd`). |
| `crates/inktick-sets` | zip and 7z, parents and clones. |
| `crates/inktick-test` | Headless runner: `png`, `wav`, `trace`, `probe`, `check`, `clockfind`, `clockcheck`, `speed`, `bench`. |
| `crates/inktick` | The program: SDL2 + GL, [nibkit](https://github.com/abradburne/nibkit) (a git dependency; CONTRIBUTING.md, "nibkit"). |
| `packaging/nibos` | `inktick.toml` (checked against `options::SPECS` by a test with `--features dslot`) and the PKGBUILD. |
| `tools/` | `run-on-device.sh`, `lib.sh` (settings from `config.local.sh`). |
| `docs/` | Design, architecture, accuracy, provenance, using. |

## Checks

```bash
make check     # fmt, clippy -D warnings (each system's build), all tests, shellcheck
```

The tests that need the units' sets skip without them. With a collection of sets at hand, a
change to the core is run against every unit (`inktick-test png` and `check`, and `clockcheck`
for the clocks) before it is committed.

## The unit database

A row's button lines are found by behaviour (`inktick-test probe`, `check`), never copied from
another emulator's input definitions. A row's clock comes from `clockfind` and is checked by
`clockcheck`. Labels are the unit's own, as printed on it or in its instructions; where only
behaviour says what a button is, the commit says so and the row waits for someone to check it
on a unit or its instructions.

## On the device

Builds run in the aarch64 build machine (`BUILD_MACHINE`). Take the shared device lock first
(nibOS's `tools/device-lock.sh take "WHO: WHAT"`, `drop` after, `game` to see if someone is
playing); never stop DSlot while a game is being played (`pgrep dsperate`, `pgrep inkdot`,
`pgrep nibcase`); start DSlot again after, in a trap. `tools/run-on-device.sh` does all of this,
bounded by `LIMIT` seconds, and mutes the run unless `SOUND=on`.
Check the battery (`nib battery`) before long runs.

## Commits

One logical change per commit, `core:`, `art:`, `sets:`, `frontend:`, `test:`, `docs:`,
`packaging:` prefixes where one part is touched; the body says why, what was measured, what was
tested. Several people and agents work in these repositories at once: commit the paths you
changed by name, never `git commit -a`. Agents add `Co-Authored-By:`, never `Signed-off-by:`
(that is the human's certification, the DCO).
