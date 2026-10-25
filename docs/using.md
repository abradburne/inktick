# Using inktick

inktick plays MAME's Game & Watch sets, `gnw_<name>.zip` or `.7z`, as MAME names them (a set
renamed isn't found). It can be started two ways:

- **From DSlot** (nibOS), for sets in `Roms/GW` on either card.
- **By hand**: `inktick gnw_dkjr.7z`, or `inktick` alone for a list of the units in `Roms/GW` on
  the cards. `inktick --help` gives the options, which are the same as on the menu's Unit page
  (`--view unit`, `--ghosting slow`, `--clock own`); `inktick --units` lists every unit inktick
  knows.

A clone whose set holds only its screens needs its parent's set beside it.
inktick finds a unit's artwork (MAME's `<set>.zip` with its `default.lay`) in a folder given by
`--artwork`, in `artwork/` beside the set, or in `Roms/artwork/GW/`. Without artwork the screens
are shown bare, on a pale reflector.

Leaving (Menu, or Save and quit in the menu) saves the unit as it is, its high scores and clock
included; the next start resumes there. The units have no other memory: on a real unit, the
batteries keep it.

## The clock

With "The unit's clock" set to "Set from the system" (the default), the unit's clock shows the
system's time: it is set on every start, on loading a state, after ACL and after the handheld
wakes. Its alarm then rings at the real time while inktick runs. "Its own" leaves the unit's
clock alone, to be set with its buttons as on the unit (a new unit starts at 12:00 AM).

## Controls

| Control | |
|---|---|
| D-pad (or the stick) | the unit's directions; on units with a button in each corner, Up and Down are the left pair |
| A, B | the action button (Jump, Hit); with only Left and Right buttons, A is Right and Y is Left too, for two thumbs |
| X, B | on corner-button units, the right pair (X upper, B lower) |
| Start | GAME A (or GAME) |
| Select | GAME B |
| L | TIME |
| R | ALARM |
| On Pinball | the left flippers on Left, L or Y; the right ones, which also launch the ball, on A, R or Right (TIME and ALARM are then on the menu's Play page) |
| Back | the menu on the bottom screen; the unit pauses; Back again to play |
| Menu | save and leave (back to DSlot's shelf) |
| Menu + R / Menu + L | quick save / quick load |

On a single-screen unit the bottom screen shows the unit's buttons, where they are on the unit,
to touch: GAME A, GAME B, TIME along the top, the controls below. On a computer one window
shows both screens, the lower under the upper, and the mouse is a finger on it; the keys are
nibkit's (the arrows, X and Z for A and B, Return for Start, Escape for Back, Home for Menu).

## The menu

| Page | |
|---|---|
| Play | resume, save and load a state, the unit's own buttons (for two-screen units, whose bottom screen is a game screen), ACL, save and quit |
| States | the six slots and the resume point, each with its picture |
| Unit, GW | the options for this unit, and for every unit (the same rows as DSlot's pages) |
| System | both screens' brightness, the volume |
| Connections | Wi-Fi and Bluetooth (nibOS) |

## Options

| Option | Values | |
|---|---|---|
| View | The screens, large · The whole unit · Plain LCD | how much of the unit is shown: the screens on their backdrop filling the panels, the unit's case around them, or the bare LCD |
| LCD response | As the unit · Slow · Instant | how quickly segments darken and fade |
| Segment shadows | Soft · Deep · Off | the segments' shadow on the reflector behind the glass |
| Segment contrast | 60-140 % | how dark a segment is |
| Sound | Piezo · Clean | the buzzer through a model of the unit's piezo, or the plain square wave |
| The unit's clock | Set from the system · Its own | above |
| Bottom screen in play | Dim when idle · Off when idle · Always lit | for single-screen units |
| Resume where I left off | on · off | |
