# Changelog

## 0.1.0 (unreleased)

The first version, a preview: 41 units play; some of their controls, found by behaviour, wait
for a check on a unit (the README lists them), and the SM5A units don't run yet.

- **Core**: the Sharp SM510, SM511 and SM512 from Sharp's data books: every instruction, the
  divider, the LCD driver latched a frame at a time, the buzzer, the SM511/512's melody ROM,
  halt and wake, ACL; stretches between events run at once; versioned save states.
- **Units**: a database of 41 units: their series, buttons (role, printed label, line, place on
  the unit) and where each keeps its clock.
- **Sets and artwork**: MAME's sets read in zip or 7z, with parents; the screens' SVGs; MAME's
  artwork layouts and pictures.
- **The LCD**: segments that darken and fade as the glass does, their shadows on the backdrop,
  the unit's artwork around them; drawn by the GPU from data prepared once, straight into the
  panel's window on the DS+.
- **Sound**: the piezo, band-limited, through a model of its resonance, or clean.
- **The clock**: each unit's set from the system's on every start, load, ACL and wake; the
  alarm rings at the real time.
- **DS+ program**: a screen per panel for two-screen units; for single-screen units the
  bottom screen holds the unit's buttons to touch; DSlot's menu on the bottom screen (states with
  pictures, options per unit and for all, brightness, volume, Connections); resume where the unit
  was left. On the DS+: 9.6 % of one core in a game, 2.2 % at the clock (Donkey Kong Jr.).
- **nibOS**: a PKGBUILD, a DSlot description whose options DSlot shows per console and game, and
  the list of units for DSlot's GW page (`inktick --units`).
