# inktick

A Game & Watch emulator, written from scratch in Rust. It emulates the Sharp SM510, SM511 and
SM512 that run 41 of Nintendo's units, plus:

- **Their LCDs**, drawn to look like the real glass: segments fade in and out, cast shadows on
  the backdrop, and sit inside the unit's artwork.
- **The piezo's sound.**
- **Each unit's clock**, set from the system's.

It is made for the Anbernic RG DS+ (nibOS, from DSlot or by itself) and runs on any computer:
the core knows nothing of the handheld. A build for the DS+'s stock OS compiles
(`--features stock`) but hasn't been tried there yet.

On the DS+:

- **A single-screen unit** fills the top screen, and its buttons are on the bottom one, to
  touch.
- **A two-screen unit** (Donkey Kong, Mario Bros., Zelda) has a screen on each panel.

**This is a preview (0.1.0).** Every unit below plays, but some of their controls were found
only by what they do and still wait for a check on a real unit or its instructions. The first
units, on the SM5A, don't run yet.

## What it plays

The players' own MAME Game & Watch sets (`gnw_dkjr.zip` or `.7z`, with their screens as SVG),
and the MAME artwork packs for the unit around them. No ROM or artwork is part of inktick.
`inktick --units` lists the units it knows; [docs/accuracy.md](docs/accuracy.md) says how each
was checked.

| Set | Unit | Model | Year | Series | Status |
|---|---|---|---|---|---|
| `gnw_bfight` | Balloon Fight (Crystal Screen) | BF-803 | 1986 | Crystal Screen | Playable |
| `gnw_bfightn` | Balloon Fight (New Wide Screen) | BF-107 | 1988 | New Wide Screen | Playable |
| `gnw_bjack` | Black Jack | BJ-60 | 1985 | Multi Screen | Playable |
| `gnw_boxing` | Boxing | BX-301 | 1984 | Micro Vs. System | Playable; controls to check |
| `gnw_bsweep` | Bomb Sweeper | BD-62 | 1987 | Multi Screen | Playable |
| `gnw_cgrab` | Crab Grab | UD-202 | 1984 | Super Color | Playable |
| `gnw_climber` | Climber (Crystal Screen) | DR-802 | 1986 | Crystal Screen | Playable |
| `gnw_climbern` | Climber (New Wide Screen) | DR-106 | 1988 | New Wide Screen | Playable |
| `gnw_dkcirc` | Donkey Kong Circus | MK-96 | 1984 | Panorama | Playable |
| `gnw_dkhockey` | Donkey Kong Hockey | HK-303 | 1984 | Micro Vs. System | Playable; controls to check |
| `gnw_dkjr` | Donkey Kong Jr. (New Wide Screen) | DJ-101 | 1982 | New Wide Screen | Playable |
| `gnw_dkjrp` | Donkey Kong Jr. (Panorama) | CJ-93 | 1983 | Panorama | Playable; up and down to check |
| `gnw_dkong` | Donkey Kong | DK-52 | 1982 | Multi Screen | Playable; up and down to check |
| `gnw_dkong2` | Donkey Kong II | JR-55 | 1983 | Multi Screen | Playable; up and down to check |
| `gnw_dkong3` | Donkey Kong 3 | AK-302 | 1984 | Micro Vs. System | Playable; controls to check |
| `gnw_fireatk` | Fire Attack | ID-29 | 1982 | Wide Screen | Playable |
| `gnw_gcliff` | Gold Cliff | MV-64 | 1988 | Multi Screen | Playable; up and down to check |
| `gnw_ghouse` | Green House | GH-54 | 1982 | Multi Screen | Playable |
| `gnw_lboat` | Life Boat | TC-58 | 1983 | Multi Screen | Playable |
| `gnw_manhole` | Manhole | NH-103 | 1983 | New Wide Screen | Playable |
| `gnw_mario` | Mario Bros. | MW-56 | 1983 | Multi Screen | Playable |
| `gnw_mariocm` | Mario's Cement Factory (New Wide Screen) | ML-102 | 1983 | New Wide Screen | Playable |
| `gnw_mariocmt` | Mario's Cement Factory (Table Top) | CM-72 | 1983 | Table Top | Playable |
| `gnw_mariotj` | Mario the Juggler | MB-108 | 1991 | New Wide Screen | Playable |
| `gnw_mbaway` | Mario's Bombs Away | TB-94 | 1983 | Panorama | Playable |
| `gnw_mickdon` | Mickey & Donald | DM-53 | 1982 | Multi Screen | Playable |
| `gnw_mmousep` | Mickey Mouse | DC-95 | 1984 | Panorama | Playable |
| `gnw_opanic` | Oil Panic | OP-51 | 1982 | Multi Screen | Playable |
| `gnw_pinball` | Pinball | PB-59 | 1983 | Multi Screen | Playable |
| `gnw_popeyep` | Popeye | PG-92 | 1983 | Panorama | Playable; RIGHT and PUNCH to check |
| `gnw_rshower` | Rain Shower | LP-57 | 1983 | Multi Screen | Playable; directions to check |
| `gnw_sbuster` | Safe Buster | JB-63 | 1988 | Multi Screen | Playable |
| `gnw_smb` | Super Mario Bros. (Crystal Screen) | YM-801 | 1986 | Crystal Screen | Playable |
| `gnw_smbn` | Super Mario Bros. (New Wide Screen) | YM-105 | 1988 | New Wide Screen | Playable; A and B to check |
| `gnw_snoopyp` | Snoopy | SM-91 | 1983 | Panorama | Playable |
| `gnw_squish` | Squish | MG-61 | 1986 | Multi Screen | Playable |
| `gnw_ssparky` | Spitball Sparky | BU-201 | 1984 | Super Color | Playable |
| `gnw_stennis` | Snoopy Tennis | SP-30 | 1982 | Wide Screen | Playable |
| `gnw_tbridge` | Turtle Bridge | TL-28 | 1982 | Wide Screen | Playable |
| `gnw_tfish` | Tropical Fish | TF-104 | 1985 | New Wide Screen | Playable |
| `gnw_zelda` | Zelda | ZL-65 | 1989 | Multi Screen | Playable; up and down to check |

**Playable** means its buttons, clock and games work, and every control does something in a
game. The notes name the controls found only by behaviour, without a picture of the unit's pad
or its instructions to confirm them. ALARM lines found by their display are listed in
[docs/accuracy.md](docs/accuracy.md).

**Not supported yet: the SM5A units.** These are the Silver and Gold series and the first Wide
Screens: Ball (`gnw_ball`), Flagman (`gnw_flagman`), Vermin (`gnw_vermin`), Fire (Silver,
`gnw_fires`), Judge (`gnw_judge`), Manhole (Gold, `gnw_manholeg`), Helmet (`gnw_helmet`), Lion
(`gnw_lion`), Parachute (`gnw_pchute`), Octopus (`gnw_octopus`), Popeye (Wide Screen,
`gnw_popeye`), Chef (`gnw_chef`), Mickey Mouse (Wide Screen, `gnw_mmouse`), Egg (`gnw_egg`),
and Fire (Wide Screen, `gnw_fire`). Their chip waits for a documentary source for its
instruction set; the clean room rules out the emulators that have one.

## The pieces

| | |
|---|---|
| [`crates/inktick-core`](crates/inktick-core) | The chips (SM510, SM511, SM512), the machine around them (divider, LCD frames, buzzer, melody), the unit database, the band-limited synthesiser, save states. `no_std`, no dependencies, no `unsafe`. |
| [`crates/inktick-art`](crates/inktick-art) | The screens and artwork drawn: the SVG reader, MAME layout files, the LCD's optics, the composer and the data the GPU draws from. |
| [`crates/inktick-sets`](crates/inktick-sets) | Reading the sets and artwork: zip, 7z (LZMA, LZMA2), parents and clones. |
| [`crates/inktick`](crates/inktick) | The program: SDL2, OpenGL ES 3, [nibkit](https://github.com/abradburne/nibkit). |
| [`crates/inktick-test`](crates/inktick-test) | The core run headless: pictures, sound to WAV, traces, the probes that found each unit's buttons and clock, benchmarks. |
| [`packaging/nibos`](packaging/nibos) | The PKGBUILD and the description DSlot reads. |

The docs:

- [docs/design.md](docs/design.md): the design, as built.
- [docs/architecture.md](docs/architecture.md): how the code is put together.
- [docs/using.md](docs/using.md): how to play.
- [docs/accuracy.md](docs/accuracy.md): what is checked, and how.
- [docs/provenance.md](docs/provenance.md): the documents it was written from. It is a clean
  room: no other emulator's source.

## Building and testing

```bash
make check                                             # format, lint, every test
cargo run --release -p inktick --no-default-features --features desktop -- gnw_dkjr.7z
```

On a computer one window shows both screens, the lower under the upper.

For the DS+ on nibOS:

- `tools/run-on-device.sh SET [ARTWORK]` builds in the aarch64 build machine and runs it on the
  handheld in place of DSlot. The run is silent unless `SOUND=on`.
- `packaging/nibos/PKGBUILD` builds the package.

The program is built for one system at a time, chosen by Cargo feature:

- `nibos` (the default);
- `stock`, the DS+'s stock OS;
- `desktop`, a computer.

`dslot` adds DSlot's part: its settings file, and a test that DSlot's description of inktick
matches inktick's options. nibkit comes from its repository at a tag; to build against a local
checkout, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

MIT OR Apache-2.0, at your option. The program has the Roboto Condensed fonts built in (through
nibkit's menu), which are under the SIL Open Font License 1.1: [LICENSE-OFL](LICENSE-OFL),
installed with every package. The crates built into the program are listed, with their
licences, in [THIRD-PARTY.md](THIRD-PARTY.md). See [CONTRIBUTING.md](CONTRIBUTING.md) for how
changes are made (the DCO, and the rules for AI-assisted contributions).
