# Provenance

Every source this project's code and data rest on, what was taken from it, and its licence
where that matters. The rules are in [design.md](design.md#clean-room-rules): Sharp's documents,
published hardware analyses, the units' printed instructions, file-format documentation and
black-box behaviour; never the source of an emulator, nor documents kept in an emulator's
repository. A commit that implements a hardware behaviour names its source in its body. Add a
row before using a new source.

## Hardware documentation

| Source | Where | Used for |
|---|---|---|
| *Sharp Microcomputers Data Book* (1990), scan | bitsavers: `components/sharp/_dataBooks/1990_Sharp_Microcomputers_Data_Book.pdf` (fetched 2026-10-22 from the UK Mirror Service copy) | SM510 (pp. 149-158) and SM511/SM512 (pp. 159-170): pins, program counter and stack, ROM and RAM maps, divider, standby, ACL, buzzer, melody, LCD driver, instruction set. SM530 (pp. 100-110), later. |
| *Sharp MOS Semiconductor Data Book* (1986), scan | bitsavers: `components/sharp/_dataBooks/1986_Sharp_MOS_Semiconductor_Data_Book.pdf` | SM-5A (pp. 74-78): features, pins, electrical data, system diagram. It has no instruction set. |
| *Sharp Microcomputer Data Book* (1996), scan | bitsavers: `components/sharp/_dataBooks/1996_Sharp_Microcomputer_Data_Book.pdf` | The SM3903's instruction set (p. 325), the SM510's in a clean scan: confirms the 1990 table, DC without a skip, T at 0x80-0xBE. The SM5L's divider naming (f15 to f0, f0 = 1 Hz) and its DTA instruction; the SM500's R1 gated "with the f1, f4 or f12 of a divider" (1990 book, p. 122). SM5L1/2/3, for later. |

## The sets

| Source | Used for |
|---|---|
| A collection of MAME's Game & Watch sets (60 zip and 7z archives), studied in a working copy 2026-10-22, never committed | File names and sizes per CPU, parent/clone layout, the SVG structure and the subset of SVG used, the segment address ranges |
| MAME's artwork packs for the same units, read in place, never committed | The layouts (views, elements, the screens' places); the clickable buttons' places and the bit each is tagged with, as a first guess at each button's line, checked by behaviour; the pictures of the units, for their buttons' printed labels (Zelda's GAME, CONTINUE, TIME, ALARM, ATTACK) |
| Mario Wiki, "Game & Watch" (mariowiki.com/G_%26_W, 2026-10-22) | That a unit's time is set with the left buttons for hours and the right for minutes, and the alarm with a recessed switch: used to tell a unit's left-side lines from its right-side ones while its clock is being set |

## Our own derivations

| Fact | Method | Status |
|---|---|---|
| SVG segment titles `x.y.z`: row x (0 = a, file R; 1 = b, file S; 2 = bs, column 0 bs1 and 1 bs2; 3 = c, file Q), column y, common z | The ranges across all 89 SVGs set against Sharp's output counts and RAM figures (SM510 Fig. 8, SM511/512 Figs. 6-7) | Confirmed: all 39 SM510/511/512 sets draw the same screens as the black box at 6 s |
| P_L's polynomial: shift right, bit 5 ← ¬(bit 0 ⊕ bit 1) | Of the 6-bit maximal sequences, the one with the lowest bigram entropy over all 37 programs; its reverse ranks second, and only this direction reads as code at the power-on (37:00) and wake (10:00) pages | Confirmed by every unit running |
| LB's B_L3,2 ← I3 ∨ I2 (Sharp's ⊕) | XOR leaves Donkey Kong's power-on display half clear; OR lights every segment after ACL, as the units do, and lets LB reach B_L = F | Confirmed |
| LAX "skip when in succession": a LAX skipped for succession counts, one skipped by a test does not | Runs of LAX entered part-way are lookup tables (Zelda 24:3C); Super Mario Bros.' clock skips a LAX by ADX's carry and needs the next one (01:31) | Confirmed: both units' clocks then match the black box |
| BP is 1 after reset | Donkey Kong never executes ATBP yet shows its LCD | Confirmed |
| ATPL keeps P_L5,4 of the ATPL's own step | Sharp: "P_L3-P_L0 ← Acc" | Not yet tested by a unit that would show otherwise |
| SM511/512 0x01 = DTA: Acc ← the divider's top four bits, f0 (1 Hz) in bit 3 | Not in Sharp's SM511 table (49 of the stated 55 listed); every use loads Acc before a bit-3 test; the SM5L's DTA. The bit order fitted to the black box: Snoopy's game sounds start when the black box's do (to 50 ms) only this way round | Confirmed on Snoopy |
| γ is set as the divider wraps (the 1 Hz signal's falling edge), a second after IDIV | Fitted to the black box: the sounds at a game's start in Fire Attack (0.21 s off on the rising edge, 0.01 s on the wrap), Tennis and Snoopy | Confirmed on three units; no unit worse |
| SM511/512 0x5D = CEND, 0x65 = IDIV | The SM510's codes; contexts: halt points before the wake page, clock setting (IDIV then TIS) | Confirmed |
| SM511/512 60 36 = 16.384 kHz, 60 37 = 8.192 kHz | 60 36 at power-on and on waking, 60 37 then idle steps before every CEND | Plausible; game tempo to be compared |
| TF1, TF4 test the 2 Hz and 16 Hz taps; the buzzer is f12 (4,096 Hz) | Sharp's naming f_n = 2^n Hz (SM500, SM5L) | Plausible. Against the black box: TF1 one tap lower changes nothing seen; TF4 one tap lower makes Green House's sounds exact but halves Donkey Kong Jr.'s 0.4 s beat, so it stays |
| The melody ROM's step: low nibble the note (2-13), 0 rest, 1 stop; bit 4 set the upper octave, bit 5 set the long tone (125 ms) | Value statistics over the 24 melody ROMs: every note code appears with both bits in all combinations; 0 and 1 only bare or with bit 5. Which value is which fitted to the black box: Snoopy's start-up tune recorded from both, its 15 notes' starts and ends (within 50 ms) and pitches (within 4 %) | Confirmed on Snoopy |
| bs flashing: a Y bit blanks its bs segment while f0 (1 Hz) is low | Sharp: "flash ... under the control of Y", "on for 0.5 sec and off for 0.5 sec" | Plausible |
| Each unit's clock: six BCD nibbles (hour, minute, second; 12-hour with PM in bit 3 or bit 1 of the hour's tens, or 24-hour) | `inktick-test clockfind`: a day untouched after TIME, sampled each second, nibbles kept that change on the clock's beats; `clockcheck`: set times either side of each carry count on correctly | Confirmed on 39 sets (docs/accuracy.md) |
| A unit's buttons by what they do | `inktick-test probe`: each line pressed from power-on, from the clock and in a game; the strobe patterns read in each | Per unit, docs/accuracy.md |

## Black box

MAME may be run as a program for internal comparison. Its source is never opened.

| Version | Obtained | Used for |
|---|---|---|
| MAME 0.289 | Arch Linux ARM's `extra/mame` binary package, installed in the build machine (2026-10-22) | Snapshots of each set at 6 s after power-on (`-seconds_to_run 6`), compared by eye with ours; `-listxml` for a set's inputs and their defaults (BA and β idle high); a Lua script pressing a button by the name its input menu gives and taking snapshots; `-wavwrite` recordings of a game's start, whose sound timing and pitch are set against ours; through its Lua console, the running machine's state as a snapshot shows it: the input ports (which control sets which bit of which port, read while pressed) and the chip's data RAM at given times, set beside ours (Mario's Bombs Away's controls, 2026-10-22). Internal engineering data, not kept or published. |

## Other

| Source | Used for |
|---|---|
| PKWARE APPNOTE.TXT | Reading zip sets and artwork (`inktick-sets/src/archive.rs`) |
| 7-Zip, "7z Format description" (DOC/7zFormat.txt, version 18.06) | Reading 7z sets: signature header, encoded header, streams, folders, sub-streams, file names |
| MAME, "MAME Layout Files" (`docs/source/techspecs/layout_files.rst`, the user documentation; read from the repository's docs folder because docs.mamedev.org did not answer) | The layout format: coordinates and bounds, colours, element components and states, view items, blend modes, orientation, collections and groups, drawing order |
| SVG 1.1 specification (W3C): paths, transforms, the elliptical arc notes F.6 | The screen reader (`inktick-art/src/path.rs`) |
| Stilson and Smith, "Alias-Free Digital Synthesis of Classic Analog Waveforms" (ICMC 1996); R. Bristow-Johnson, "Audio EQ Cookbook" | The band-limited steps and the piezo's band-pass |
| crates.io API, AUR RPC, GitHub repository search, web search (2026-10-22) | Name collision checks |
| inkdot (our own project): `docs/design.md`, `docs/architecture.md`, `AGENTS.md` | The project's shape, the band-limited synthesiser's design, the resume header |
| [nibkit](https://github.com/abradburne/nibkit) (nibOS's SDK library, our own, MIT) | Screens, vblank, input, touch, audio advice, paths, settings, the shared menu, DSlot's description format |
| DSlot `docs/emulators.md`, `docs/game-watch.md`, `src/resume.rs` (our own launcher) | The saves-folder rule, SIGTERM and resume, resume formats, the GW page |
| Skipped unopened (forbidden by the rules) | Search results pointing into MAME's source, Yokoi, an FPGA core's `docs/instructions.md`, an SM-5A debugger's site, a Habr article on building an emulator |
