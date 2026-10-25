# Accuracy

What is checked, how, and what isn't yet. The units' sets are the players' own and are never in
the repository, so the checks that need them run where the sets are (`inktick-test`), and their
results are recorded here.

## The chips

- **Per-instruction tests** (`crates/inktick-core/tests/instructions.rs`, 29 tests, no ROMs):
  small programs assembled by `asm.rs` exercise each instruction group as Sharp's tables describe
  it: arithmetic and its skips, the stack, the P_L polynomial across pages, LAX's skip in
  succession, the divider's taps and TIS, the buzzer's gating, the melody's steps, the LCD's
  latching, halt and wake, ACL; and that the machine's stretches run exactly as cycle by cycle.
- **Every SM510, SM511 and SM512 unit against the black box**: each of the 39 sets in the
  collection used, started and left 6 s, draws the same segments as MAME 0.289 run as a
  program (compared by eye; internal engineering data, not kept). Where the two first differed,
  the provenance's derivations say what was found and how.

## The units' clocks

`inktick-test clockfind` runs a unit a day, untouched after TIME, and finds the RAM nibbles that
count on the clock's beats; `clockcheck` sets each unit's clock either side of its carries
(23:59:57, 11:59:56, 12:59:55, 0:59:58, 9:59:59, 21:09:05), runs it on and reads the time back,
at its clock and inside a game, and draws the set time. Last run 2026-10-22, every set in the
collection:

| | |
|---|---|
| Units with a clock entry | 41 of 41 (two clones by their parent's program, unrun: not in the collection) |
| 12-hour, PM in bit 3 of the hour's tens | 26 |
| 12-hour, PM in bit 1 | 14 |
| 24-hour | 1 (Black Jack) |
| Counting on correctly from each set time | 39 of 39 sets run |
| Shown correctly (10:42 PM drawn) | the six looked at (Donkey Kong, Black Jack, Mickey & Donald, Balloon Fight, Manhole, Donkey Kong II) |

Set at an arbitrary point in a second, a unit's next second comes up to a second early or late,
so a set clock is right to within a second.

## The units' buttons

A line comes from the artwork's clickable button where the artwork has one, and is checked, or
found, by behaviour on our core (`inktick-test probe` and `check`): from power-on, from the
clock and from inside a game, which line does what. The labels are the units' own,
from pictures of them. Within a series the order of the GAME A, GAME B, TIME and ALARM lines
repeats unit after unit, which names the lines of units whose pictures aren't at hand.

| | |
|---|---|
| Units with TIME and their games | 41 of 41 |
| Units with their controls | 41 of 41, each control doing something in a game (tried 3, 6, 9 and 15 s in, for the units with a long opening) and none, held, putting the unit in its all-segments display (`inktick-test controlcheck`) |
| Found by the pad's order, not yet seen moving | Up and Down of Donkey Kong, Donkey Kong II, Donkey Kong Jr. (Panorama), Gold Cliff, Zelda, Rain Shower; Left and Down of Rain Shower |
| Checked against MAME's RAM (black box) | Mario's Bombs Away: UP/DOWN, LEFT and RIGHT change the same RAM nibbles in both (its game reads them only after a 13 s opening) |
| To check on a unit or its instructions | Super Mario Bros. (New Wide Screen): which of its game lines is A and which B; Popeye (Panorama): Right and PUNCH, read from his poses; Boxing, Donkey Kong 3 and Donkey Kong Hockey (Micro Vs. System): which player's pad these lines are (the one at the right of the screen), Boxing's UP and DOWN (a high and a low punch), Donkey Kong Hockey's SHOOT; the ALARM lines found by their display (a PM 12:00 alarm time) |

## The picture and the sound

- **The LCD on the GPU** is checked against the CPU's composer, pixel for pixel: within 1 in
  255 on a computer, and on the DS+ read back from the panel's window (Donkey Kong Jr. in a
  game, 2026-10-22).
- **The synthesiser** has unit tests: the band-limited step's shape, a square wave's level, a
  tone dying away into exact silence and sounding again.
- **Not yet compared**: most units' sound against the black box (the starts of Snoopy, Fire
  Attack and Snoopy Tennis were: provenance.md), and golden pictures kept as tests.
