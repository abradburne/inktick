# Architecture

## The core (`crates/inktick-core`)

```
Machine::run_until(tick)               machine time in crystal cycles (32.768 kHz)
   └─ per stretch (Machine::ticks_for):
        Chip::step()                   one instruction: 1 or 2 cycles (chip.rs, exec_510/511)
        the divider                    15 bits; f_n = 2^n Hz taps for TF1, TF4, the second
        the LCD                        a common every 128 cycles; a frame (64 Hz) is latched
                                       into Segments when its last common is driven
        the buzzer                     R gated by f12 (SM510) or the melody (SM511/512):
                                       each level change kept as an edge (tick, level)
```

- **Stretches.** A halted chip, or one between beats, changes nothing until the next LCD common,
  the next one-second change or the next buzzer edge, so `ticks_for` jumps to whichever comes
  first; the first cycle is always run alone, so an edge is never late. A test runs the same
  program both ways and compares registers, segments and edges.
- **The chips** are one `Chip` with a `Kind`: what the three share is `exec_common`, the SM510's
  and the SM511/512's own codes are apart. The program ROM is the 4096-byte address space
  P_U:P_M:P_L, as the sets hold it; P_L counts by Sharp's polynomial (provenance lists how it was
  found).
- **Inputs** are `Lines`: a K nibble for each strobe S1-S8, and BA and β. The units' wiring is
  in the database (`units.rs`, rows in `units_table.rs`): each button's role, label, line and
  place on the unit, and where the unit keeps its clock (six BCD nibbles, 12-hour with a PM bit
  or 24-hour), so `Unit::set_time` can set it.
- **Sound**: `synth::Synth` places a band-limited step at each edge's exact fractional sample,
  then a DC-blocking high-pass and the piezo's band-pass (RBJ). Silence in and settled filters
  give silence out without running them.
- **Save states**: each part has one `visit` function that both saves and loads (`state.rs`).

## The art (`crates/inktick-art`)

A set's screens are SVGs whose shapes are titled with their segment's address (`x.y.z`: row,
column, common). The artwork is a MAME layout (`default.lay`) with its pictures. `Scene` lays a
view out for an output size: the artwork's items in order, the screens where the view puts them,
each segment's coverage mask; `optics.rs` turns the segments' on and off into each segment's
darkness over time (rise 25 ms, fall 70 ms, as the unit's glass), and the shadow on the
reflector behind.

A segment's darkness L gives its pixels by an exact cubic in t = 3L, kept as three Newton
differences per pixel in fixed point (`Scene::lcd`). The CPU composer and the GPU use the same
data, so a screenshot and the screen agree (to 1 in 255).

## The program (`crates/inktick`)

```
main loop:  wait for the top panel's vblank (nibkit::vblank)
            read input (SDL, the touch panel): roles, through the unit's lines   input.rs
            run the machine to real time (a stall, a suspend, isn't caught up:
                 the clock is set again from the system's)                         main.rs
            the synthesiser: the edges into 48 kHz samples, queued to SDL
            the optics: on a switch at once, a fade every other refresh           stage.rs
            the GPU: the off picture, then every segment's quad at its level
                 (two draws: ADD and REVERSE_SUBTRACT), straight into the window  video.rs
            the bottom screen: the unit's buttons or the menu, when they change   ui/
            a resume point or a state: written by a thread                        saves.rs
```

Nothing is drawn or swapped while nothing changes: a unit showing the time draws about once a
second. The options are declared once, `options::SPECS`: the command line, the menu's Unit and
GW pages and the settings file all come from it; `packaging/nibos/inktick.toml` is DSlot's
description of the same options, and a test (with `dslot`) holds the two together.

A save is `ITKS` 1: made and played (u64 LE seconds), a PNG of the screens (256 × 192 each,
stacked), then the machine. `<set>.auto.state` is the resume point, written on leaving and read
on starting; states 1-6 are the menu's slots.

## Known gaps

- **Controls** are known for all 41 units (docs/accuracy.md lists what waits for a check on a
  unit).
- **The SM5A** (19 units) waits for a description of its instruction set; the SM530 later.
- **Sets must keep their MAME names**: the screens are found inside the set by its name.
