# Design

inktick emulates the Sharp SM5xx 4-bit microcontrollers and the LCD handhelds built on them,
starting with the Nintendo Game & Watch range. It runs the units' own ROMs, so every game
behaves as the hardware does: its timing, its sounds, its clock and alarm, its quirks. This
document is the design as built. [architecture.md](architecture.md) maps it onto the code.

## Goals

In order:

1. **Faithful**: the CPU and LCD as Sharp documented them, checked by tests.
2. **Beautiful**: the LCD looks like an LCD at any size, with its shadows and its slow fade.
3. **Frugal**: the CPU is asleep almost all the time.
4. **Complete as a product**: the clock keeps time, the alarm rings, resume works, states have
   pictures, and both screens are used.

It is not tied to one device:

- **The core** (`inktick-core`) is the machine and the unit database. It is `no_std`, has no
  dependencies and no `unsafe`, and does no I/O. Given a program and button states, it produces
  segment states and sound edges, so anything can host it.
- **The program** (`inktick`) is built on nibkit, for nibOS (the default), the RG DS+'s stock OS
  or a computer, chosen by Cargo feature.

Not supported yet:

- **The SM5A**, which waits for a documentary source (below).
- **The SM530 and SM5L families.**
- **Non-Sharp CPUs.**
- **A libretro build.**

## Clean-room rules

inktick is written so that its authors own all of it and can license it freely.

**Sources allowed:**
- Sharp's data books and manuals.
- Published hardware analyses, as documents.
- The units and their printed instructions.
- File-format documentation, including MAME's *user documentation* of its layout files.
- General literature: the SVG and zip specifications, DSP texts.
- Black-box behaviour.

**Never read:** the source of any emulator or emulator component, or a document kept in one's
repository. This covers MAME's source (its `sm510` devices and `hh_sm510` drivers included),
Yokoi, PolluxPT's port, FPGA cores (their `docs/` folders included: they document an
implementation), gw-libretro, LCD-Game-Emulator, and any port, snippet or forum post that pastes
their code. Search results that link into them are skipped unopened.

**Black box:** MAME may be run as a program, and how it behaves compared with ours: its screen,
its sound, its input ports and its RAM, as its Lua console shows them while it runs. It is used
as internal engineering data only. Its source is never opened, and no comparison is published.

**Our own derivations:** where Sharp's documents leave a fact out, it may be derived from the
ROMs themselves or from experiment on our own core. The reasoning is written down in
[provenance.md](provenance.md). Examples:
- the order in which the 6-bit program counter steps through a page;
- what a code missing from Sharp's table does.

**Stop rule:** if a needed fact is available only in an emulator's source, work on it stops.
The SM5A is one such gap.

**Provenance log:** [provenance.md](provenance.md) lists every source used and what was taken
from it. A commit that implements a hardware behaviour names its source in its body.

**Nothing copyrighted ships:** no ROMs, screen SVGs, backdrops or bezels are in the repository
or the package. Players supply them; tests that need them skip without.

### The SM5A gap

The first Game & Watch units, the Silver and Gold series and the first Wide Screens, run on the
SM5A. Their ROMs are 1,856 bytes, against 4,096 for the later chips.

Sharp's 1986 data book describes the chip but gives no instruction set, and no other Sharp
document found has one. The Soviet KB1013VK1-2 is a licensed equivalent; its technical
description has not been found online. The only complete public tables are inside emulator
projects, which the rules forbid. The core is shaped so the SM5A can slot in when a source is
found: same lineage, 1/2-duty LCD, the same divider and cycle.

## Input: MAME's sets, read directly

Players supply the units as MAME distributes them, with no conversion.

**The set** is an archive named for the unit (`gnw_dkong.zip` or `gnw_dkong.7z`). It holds the
program ROM, a melody ROM for SM511/512 units, and one SVG per LCD:

| Chip | Files | Example |
|---|---|---|
| SM510 | a 4,096-byte ROM | `dk-52`, `gnw_dkong_top.svg`, `gnw_dkong_bottom.svg` |
| SM511/512 | `<label>.program` (4,096 bytes) and `<label>.melody` (256 bytes) | `zl-65.program`, `zl-65.melody` |
| SM5A (not run yet) | a 1,856-byte ROM | `ac-01`, `gnw_ball.svg` |

A ROM file is the whole address space: 4,096 bytes is 64 pages of 64 steps, of which Sharp's
figures count 63 in use. Two-screen units name their SVGs `_top`/`_bottom`, or `_left`/`_right`
when side by side (Life Boat, Mario Bros., Rain Shower).

**Parents and clones:** a clone's archive may hold only its SVG, with the ROM in the parent's.
The database records each unit's parent, and the loader looks in the unit's own archive, then
its parent's, in the same folder.

**Archives:** zip (stored and deflate, from PKWARE's APPNOTE) and 7z (the container from 7-Zip's
published format description; LZMA and LZMA2 by `lzma-rs`). This lives in `inktick-sets`, not
the core.

**Artwork** is optional: MAME's artwork archive of the same name, holding the backdrop and
bezel pictures and a layout file (`default.lay`) that places them. The layout format is read
from MAME's layout documentation. Without artwork the screens are drawn bare, on a pale
reflector.

### The unit database

The database is built in, one row per unit (`units_table.rs`). Each row holds:

| Field | Example | From |
|---|---|---|
| Set name, SHA-1 of the program | `gnw_dkong` | the set |
| Title, model number, series, year | Donkey Kong, DK-52, Multi Screen, 1982 | public catalogues, the units' printed instructions |
| Chip, parent | SM510 | the ROM's size and labels, confirmed by running |
| Buttons: role, printed label, line (strobe S and input K, or BA/β), place on the unit | Left, LEFT, S2 K4 | labels from the unit; lines from our own probes |
| Clock: where the unit keeps its time | six BCD nibbles, 12-hour with the PM bit | our own probe |

The probes find a unit's wiring and clock without any outside source (`inktick-test`):

- **`probe` and `controlfind`** log which strobe lines the program drives when it reads K and
  BA/β. They then press each live (S, K) pair from the clock and from inside a game, and record
  what changes on the LCD.
- **`clockfind`** runs a unit for a day at its clock and keeps the RAM nibbles that count on the
  clock's beats.
- **`controlcheck` and `clockcheck`** check every row: each control does something in a game;
  none, held, puts the unit in its all-segments display; the clock counts on from times set
  either side of its carries.

A person names the lines from the unit's printed labels. Where only behaviour says what a
button is, [accuracy.md](accuracy.md) lists it as waiting for a check on a unit.

### Screen SVGs

The SVGs are traced artwork: each segment is a group whose `<title>` is three numbers, `x.y.z`.
For the 4,096-byte units these are the output row (a, b, bs or c), the column (0-15) and the
common (H1-H4). This matches Sharp's output and RAM figures; the reading is our own derivation,
confirmed by every unit drawing the same screens as the black box.

The files use a small subset of SVG: `path` (moves, lines, cubic curves, arcs), `g`, `rect`,
`polygon`, transforms on groups, fills by `style`, and fill opacity. Our own reader turns that
into a flat list of segments, using `roxmltree` for the XML. Each segment is a set of filled
paths with its address, rasterised by `tiny-skia`'s anti-aliased filler.

## The core

**Timing:** the machine counts crystal cycles (32.768 kHz). One instruction takes one or two
system-clock cycles. The divider, the 1-second flag, the buzzer, the melody generator and the
LCD frames advance in step with the CPU.

`CEND` halts the chip until the 1-second signal or a key wakes it. Between events the machine
jumps straight to the next one (an LCD common, a divider change, a buzzer edge), so a unit
showing the time costs almost nothing.

**CPUs:** one `Chip` with a `Kind` (SM510, SM511, SM512). The three share most instructions;
each kind's own codes are apart. The program counter steps through a page by Sharp's polynomial
counter, whose polynomial was derived from the ROMs (provenance.md).

**LCD:** the display RAM drives the segments through the commons H1-H4 (1/4 duty), with the bs
outputs and the BP/BC bits that show or blank the whole display. A frame (64 Hz) is latched into
the segment states when its last common is driven.

**Buzzer and melody:**
- On the SM510, the piezo outputs are the R register gated by the divider's 4,096 Hz tap. On the
  SM511/512 the melody generator plays notes from the melody ROM.
- The core records each output edge at its exact cycle.
- The program's synthesiser places a band-limited step at each edge's fractional sample, then
  filters it through a DC blocker and a band-pass for the piezo's resonance ("Piezo"), or plays
  it plain ("Clean").

**Save states:** one `visit` function per part both saves and loads. A state holds the chip, RAM,
divider, buzzer and melody, a few hundred bytes, versioned.

## The picture

Each segment's darkness follows its drive through a first-order response, with separate rise
and fall times: 25 ms and 70 ms, as the glass behaves. This gives the ghosting of a real unit as
a figure moves.

The picture is composed in three layers:
1. the backdrop, from the artwork or our own;
2. a soft shadow of each dark segment on the reflector behind the glass;
3. the segments themselves, at the chosen contrast.

The artwork's bezel goes over all, in the views that show it. The GPU draws the segments from
data prepared once per size. A CPU composer, used for screenshots and tests, gives the same
picture to within 1 in 255.

Nothing is drawn while nothing changes: a unit showing the time redraws about once a second.

## The program

**Screens:**

| Unit | DS+ | Computer |
|---|---|---|
| Two screens (Multi Screen: Oil Panic, Donkey Kong, Zelda and the rest) | a screen on each panel, at the same scale | one window, the lower screen under the upper |
| One screen | the top panel; the unit's buttons on the bottom panel, to touch | one window, with the buttons |

Each screen is fitted to its own aspect, centred and never stretched. The views are "The
screens, large", "The whole unit" (with the case from the artwork) and "Plain LCD".

**Controls** map by what a button does: directions on the D-pad, the action on A, GAME A on
Start, GAME B on Select, TIME on L, ALARM on R. Units whose two controls are left and right
buttons also take Y and A, for two thumbs. [using.md](using.md) has the full table.

**Time:** with the clock "Set from the system" (the default), the unit's clock digits are
written into RAM on every start, on loading a state, after ACL and after the handheld wakes. The
unit then keeps its own time from its divider, as a real one does, and its alarm rings at the
real time while the program runs.

**States and resume:**
- Leaving (Menu, or Save and quit) writes the resume point, `<set>.auto.state`, in the game's
  saves folder; the next start resumes from it.
- Six numbered states sit beside it.
- Each is `ITKS` version 1: the time made and the play time, a PNG of the screens (256 × 192
  each, stacked), then the machine. DSlot reads the header for its shelf.
- The units have no battery RAM; the resume point is the unit's memory.

**Pacing:** each loop waits for the top panel's vblank, runs the machine to real time, draws
only if something changed, and sleeps. A stall or a suspend isn't caught up: the clock is set
again from the system's. On the DS+ it takes 9.6 % of one core in a game and 2.2 % at the clock
(Donkey Kong Jr.).

**The menu** is nibkit's shared menu, on the bottom screen:
- the Play page: states, the unit's own buttons for two-screen units, ACL;
- the States page;
- the options for this unit and for every unit;
- brightness and volume;
- Connections on nibOS.

## Integration with DSlot

- **Description:** `packaging/nibos/inktick.toml`, installed as
  `/usr/share/dslot/emulators/inktick.toml`, gives DSlot the `GW` system, the options it shows,
  and "Clear resume state". A test checks it declares exactly the options the program takes.
- **The units' list:** `inktick --units` writes the list of units, installed as
  `/usr/share/inktick/units.tsv`. DSlot names GW games by it, and starts only these sets in
  inktick.
- **Box art** stays DSlot's, keyed by the set's name.

## Testing

- **Per instruction:** small programs, written with the core's own assembler (`asm.rs`),
  exercise each instruction group as Sharp's tables describe it (`tests/instructions.rs`).
  Another test checks that the machine's stretches run exactly as cycle by cycle.
- **Hardware blocks:** the divider and the 1-second flag, CEND and wake, the buzzer's gating, the
  melody's steps, and the LCD's latching each have their own tests.
- **The database:** every row's buttons and clock are tested in the code. With the sets at hand,
  `controlcheck` and `clockcheck` run every unit.
- **The art and set readers, the synthesiser, the frontend's options and DSlot's description**
  are tested too.
- **Black box:** comparison with MAME, as above; results are recorded in
  [accuracy.md](accuracy.md) and [provenance.md](provenance.md), never the pictures themselves.

## Later

- The SM5A, once documented.
- The SM530 and SM5L1-3, for later LCD games (in Sharp's 1990 and 1996 books).
- Other makers' SM5xx games, as database rows.
- A libretro core of the same core.
- A nibOS wake alarm for the units' alarms.
