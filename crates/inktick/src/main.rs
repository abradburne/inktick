//! inktick: a Game & Watch emulator (Sharp SM510-family LCD games), for the Anbernic RG DS+ on
//! nibOS or the stock OS, started by DSlot, from the stock OS's Apps or by hand, and for
//! computers. A unit's screen fills the top panel (a two-screen unit has one per panel); the
//! bottom touch screen is its buttons, or its lower screen. `inktick --help` lists the options;
//! docs/using.md says how to play.

mod args;
mod input;
mod options;
mod perf;
mod picker;
mod saves;
mod stage;
mod ui;
mod video;

use inktick_core::units::{Role, Unit};
use inktick_core::{Machine, Piezo, Synth, CRYSTAL_HZ};
use inktick_sets::Set;
use input::{Command, Controls, Pad};
use nibkit::screens::Screen;
use nibkit::settings::Settings;
use options::Options;
use saves::{SaveFiles, Slot, Writer, SLOTS};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::controller::{Axis, Button, GameController};
use sdl2::event::Event;
use stage::{Stage, ViewChoice};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use ui::{Action, Nav, OptionRow, SlotInfo, Ui};
use video::Video;

/// Set by SIGTERM: DSlot (or systemd) asks the unit to be put away.
static TERM: AtomicBool = AtomicBool::new(false);

extern "C" fn on_term(_: libc::c_int) {
    TERM.store(true, Ordering::SeqCst);
}

fn watch_sigterm() {
    // SAFETY: a handler that only stores to an atomic.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_term as extern "C" fn(libc::c_int) as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut());
    }
}

/// The sound queue's longest, in samples: a tenth of a second. More than that (after a stall)
/// is dropped rather than heard late.
const AUDIO_MOST: u32 = 4800;
/// The bottom screen dims (or goes off) after this long untouched in play.
const IDLE: Duration = Duration::from_secs(10);
/// How long a press from the menu (Press GAME A) or ACL holds its line.
const PRESS: Duration = Duration::from_millis(300);
/// The settings' and saves' name for the system.
const CONSOLE: &str = "GW";

/// INKTICK_DEBUG, comma separated: `shot=DIR` saves both screens as PNG files there after
/// `at=SECONDS` (default 5); `press=ROLE@SECONDS` holds one of the unit's buttons then
/// (`press=GameA@3`, repeatable); `quit=SECONDS` leaves then, as SIGTERM would; `menu=PAGE`
/// opens the menu on that page at `at`. For checking a build headless and on the handheld.
#[derive(Clone, Debug, Default)]
struct Debug {
    shot: Option<PathBuf>,
    at: f64,
    presses: Vec<(Role, f64)>,
    quit: Option<f64>,
    menu: Option<String>,
    done: bool,
    /// `stats`: where each second's time went, every five seconds.
    stats: bool,
    /// `skip=lcd` or `skip=present`: leave that out, to measure the rest.
    skip: String,
    /// `mute`: no sound device is opened (test runs on a handheld someone may be near).
    mute: bool,
    /// `input`: every button event, as SDL gives it.
    input: bool,
    /// `quiet`: the sound made and queued as ever, but as silence; its loudest sample is
    /// logged each second (checking sound on a handheld without being heard).
    quiet: bool,
}

impl Debug {
    fn from_env() -> Debug {
        let mut d = Debug {
            at: 5.0,
            ..Debug::default()
        };
        for part in std::env::var("INKTICK_DEBUG")
            .unwrap_or_default()
            .split(',')
        {
            match part.split_once('=') {
                Some(("shot", dir)) => d.shot = Some(PathBuf::from(dir)),
                Some(("at", t)) => d.at = t.parse().unwrap_or(5.0),
                Some(("quit", t)) => d.quit = t.parse().ok(),
                Some(("menu", page)) => d.menu = Some(page.to_owned()),
                None if part == "stats" => d.stats = true,
                None if part == "mute" => d.mute = true,
                None if part == "input" => d.input = true,
                None if part == "quiet" => d.quiet = true,
                Some(("skip", what)) => d.skip = what.to_owned(),
                Some(("press", p)) => {
                    if let Some((role, t)) = p.split_once('@') {
                        let role = match role {
                            "GameA" => Some(Role::GameA),
                            "GameB" => Some(Role::GameB),
                            "Game" => Some(Role::Game),
                            "Time" => Some(Role::Time),
                            "Alarm" => Some(Role::Alarm),
                            "Left" => Some(Role::Left),
                            "Right" => Some(Role::Right),
                            "Up" => Some(Role::Up),
                            "Down" => Some(Role::Down),
                            "Action" => Some(Role::Action),
                            _ => None,
                        };
                        if let (Some(r), Ok(t)) = (role, t.parse()) {
                            d.presses.push((r, t));
                        }
                    }
                }
                _ => {}
            }
        }
        d
    }
}

/// What the bottom screen's backlight is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backlight {
    Lit,
    /// Dimmed to the floor; the percentage to go back to.
    Dim(u8),
    Off,
}

struct App {
    machine: Machine,
    unit: &'static Unit,
    set: Set,
    options: Options,
    files: SaveFiles,
    writer: Writer,
    video: Video,
    audio: Option<AudioQueue<i16>>,
    /// The loudest sample since the last log (`INKTICK_DEBUG=quiet`).
    loudest: u16,
    synth: Synth,
    samples: Vec<f32>,
    pcm: Vec<i16>,
    controls: Controls,
    /// The bottom screen's buttons, for a single-screen unit.
    touch: Option<ui::controls::Controls>,
    ui: Ui,
    model: ui::Model,
    stage: Stage,
    quick_slot: usize,
    played_before: u64,
    started: Instant,
    last_touch: Instant,
    backlight: Backlight,
    toast_until: Option<Instant>,
    quit: bool,
    /// Machine time follows real time from this pair: an instant and the machine's tick then.
    origin: (Instant, u64),
    last_frame: Instant,
    /// A unit button the menu pressed, until when.
    pressing: Option<(Role, Instant)>,
    acl_until: Option<Instant>,
    /// The top picture needs drawing in full again (after the menu's overlay).
    redraw: bool,
    debug: Debug,
    began: Instant,
    /// INKTICK_DEBUG=stats: time spent running the unit, on optics, composing, uploading and
    /// presenting, and frames presented, since `since`.
    spent: [Duration; 6],
    presented: u32,
    since: Instant,
    /// The segments the optics last moved towards, when, and which refresh this is.
    shown: inktick_core::Segments,
    since_optics: Instant,
    odd: bool,
}

fn main() {
    let words: Vec<String> = std::env::args().skip(1).collect();
    if words.iter().any(|w| w == "--help" || w == "-h") {
        print!("{}", args::usage());
        return;
    }
    // For DSlot's GW page (installed as /usr/share/inktick/units.tsv): the units inktick plays.
    if words.iter().any(|w| w == "--units") {
        print!("{}", units_list());
        return;
    }
    let args = match args::parse(&words) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("inktick: {e}\n\n{}", args::usage());
            std::process::exit(2);
        }
    };
    if let Err(e) = run(args) {
        eprintln!("inktick: {e}");
        std::process::exit(1);
    }
}

/// Where artwork may be: the folders given, then `artwork` beside the set, and the set's own
/// folder (an artwork archive is named like the set, so it is told apart by holding a layout).
fn artwork_dirs(args: &args::Args, set: &Path) -> Vec<PathBuf> {
    let mut dirs = args.artwork.clone();
    if let Some(d) = set.parent() {
        dirs.push(d.join("artwork"));
        if let Some(up) = d.parent() {
            dirs.push(up.join("artwork").join(CONSOLE));
        }
    }
    dirs
}

/// Every unit as a line of DSlot's list: set, title, model, year, series. A title several units
/// share (Donkey Kong Jr. in New Wide Screen and Panorama) has the unit's series after it, so the
/// shelf's lists, which show titles alone, tell them apart.
fn units_list() -> String {
    let all = inktick_core::units::all();
    let mut out = String::new();
    for u in all {
        let shared = all.iter().filter(|o| o.title == u.title).count() > 1;
        let title = if shared {
            format!("{} ({})", u.title, u.series.name())
        } else {
            u.title.to_owned()
        };
        out += &format!(
            "{}\t{title}\t{}\t{}\t{}\n",
            u.set,
            u.model,
            u.year,
            u.series.name()
        );
    }
    out
}

fn run(args: args::Args) -> Result<(), String> {
    watch_sigterm();
    let sdl = sdl2::init()?;
    let video_sys = sdl.video()?;
    let mut video = Video::open(&video_sys)?;
    let mut events = sdl.event_pump()?;
    let pads = sdl.game_controller()?;
    for file in nibkit::input::CONTROLLER_MAPPINGS {
        let _ = pads.load_mappings(file);
    }
    let _pads: Vec<GameController> = (0..pads.num_joysticks().unwrap_or(0))
        .filter_map(|i| pads.open(i).ok())
        .collect();
    if std::env::var("INKTICK_DEBUG").is_ok_and(|d| d.split(',').any(|p| p == "input")) {
        eprintln!(
            "inktick: {} joysticks, controllers opened: {:?}",
            pads.num_joysticks().unwrap_or(0),
            _pads.iter().map(|p| p.name()).collect::<Vec<_>>()
        );
    }
    let mut panel = nibkit::touch::Panel::open();
    let path = match &args.set {
        Some(p) => p.clone(),
        None => match pick(&mut video, &mut events, &mut panel) {
            Some(p) => p,
            None => return Ok(()),
        },
    };
    let name = inktick_sets::set_name(&path);
    let unit = inktick_core::units::by_set(&name)
        .ok_or_else(|| format!("{name}: not a unit inktick knows"))?;
    let set = inktick_sets::open(&path, unit.parent, &artwork_dirs(&args, &path))?;
    let options = Options::new(
        CONSOLE,
        &name,
        args.options.clone(),
        Settings::load(options::settings_file()),
    );
    eprintln!(
        "inktick: {} ({}, {}), {}, artwork {}, settings {}",
        unit.title,
        unit.model,
        set.kind.name(),
        set.screens.len(),
        set.artwork
            .as_ref()
            .map_or("none".into(), |a| a.path.display().to_string()),
        options.settings.file().display()
    );
    let machine = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Default::default(),
    );
    let dir = args
        .saves
        .clone()
        .unwrap_or_else(|| saves::default_dir(CONSOLE, &name));
    let files = SaveFiles::new(dir, &name);

    let debug = Debug::from_env();
    let audio = (!debug.mute)
        .then(|| sdl.audio().ok())
        .flatten()
        .and_then(|a| {
            let spec = AudioSpecDesired {
                freq: Some(nibkit::audio::ADVICE.rate as i32),
                channels: Some(1),
                samples: Some(nibkit::audio::ADVICE.buffer_frames as u16),
            };
            let q: Result<AudioQueue<i16>, _> = a.open_queue(None, &spec);
            q.inspect_err(|e| eprintln!("inktick: no sound: {e}")).ok()
        });
    if let Some(q) = &audio {
        q.resume();
    }
    let rate = audio
        .as_ref()
        .map_or(nibkit::audio::ADVICE.rate, |q| q.spec().freq as u32);
    let look = look_of(&options);
    let stage = Stage::new(&set, ViewChoice::parse(&options.str("view")), look);
    let touch = (!stage.two_panels()).then(|| ui::controls::Controls::new(unit));
    let mut app = App {
        machine,
        unit,
        set,
        synth: Synth::new(rate, piezo_of(&options)),
        options,
        files,
        writer: Writer::start(),
        video,
        audio,
        loudest: 0,
        samples: Vec::new(),
        pcm: Vec::new(),
        controls: Controls::default(),
        touch,
        ui: Ui::new(),
        model: ui::Model::default(),
        stage,
        quick_slot: 1,
        played_before: 0,
        started: Instant::now(),
        last_touch: Instant::now(),
        backlight: Backlight::Lit,
        toast_until: None,
        quit: false,
        origin: (Instant::now(), 0),
        last_frame: Instant::now(),
        pressing: None,
        acl_until: None,
        redraw: true,
        debug,
        began: Instant::now(),
        spent: [Duration::ZERO; 6],
        presented: 0,
        since: Instant::now(),
        shown: Default::default(),
        since_optics: Instant::now(),
        odd: false,
    };
    // Resume where the unit was left, unless told not to; else start it, at its clock.
    let mut resumed = false;
    if app.options.bool("resume") {
        if let Some(slot) = Slot::read(&app.files.state(0)) {
            if app.machine.load(&slot.machine) {
                app.played_before = slot.played;
                resumed = true;
            } else {
                eprintln!("inktick: not resuming: the resume point is for another unit or version");
            }
        }
    }
    if !resumed {
        app.unit.show_clock(&mut app.machine);
    }
    app.restart_clock();
    app.stage.snap(app.machine.segments());
    app.give_lcds()?;
    app.refresh_model();
    let vblank = nibkit::vblank::Vblank::open().ok();
    let play_clamp = perf::PLAY;
    perf::clamp_min(play_clamp);
    let mut clamped_for_menu = false;
    let mut pointers = Vec::new();
    let (mut turns, mut longest, mut since, mut last) =
        (0u32, Duration::ZERO, Instant::now(), Instant::now());
    while !app.quit {
        // Once a refresh: the panel's vblank where there is one, else a sixtieth of a second.
        match &vblank {
            Some(v) if v.wait(Screen::Top).is_ok() => {}
            _ => std::thread::sleep(
                Duration::from_micros(16_667).saturating_sub(app.last_frame.elapsed()),
            ),
        }
        if app.debug.input {
            turns += 1;
            longest = longest.max(last.elapsed());
            last = Instant::now();
            if since.elapsed() >= Duration::from_secs(1) {
                eprintln!(
                    "inktick: {turns} turns a second, the longest {longest:?}; loudest sample {}, queued {}",
                    std::mem::take(&mut app.loudest),
                    app.audio.as_ref().map_or(0, |q| q.size() / 2)
                );
                (turns, longest, since) = (0, Duration::ZERO, Instant::now());
            }
        }
        for event in events.poll_iter() {
            app.event(event);
        }
        if app.ui.menu != clamped_for_menu {
            clamped_for_menu = app.ui.menu;
            perf::clamp_min(if clamped_for_menu {
                perf::MENU
            } else {
                play_clamp
            });
        }
        if let Some(panel) = &mut panel {
            pointers.clear();
            panel.poll(&mut pointers);
            for p in pointers.drain(..) {
                match p {
                    nibkit::touch::Pointer::Down(x, y) => app.touch(x, y, true),
                    nibkit::touch::Pointer::Move(x, y) => app.touch(x, y, false),
                    nibkit::touch::Pointer::Up => app.untouch(),
                }
            }
        }
        if TERM.load(Ordering::SeqCst) {
            app.quit = true;
            break;
        }
        app.frame();
    }
    app.leave();
    Ok(())
}

fn look_of(o: &Options) -> inktick_art::Look {
    stage::look(&o.str("ghosting"), &o.str("shadow"), o.int("ink"))
}

fn piezo_of(o: &Options) -> Piezo {
    match o.str("sound").as_str() {
        "clean" => Piezo {
            amount: 0.0,
            ..Piezo::DEFAULT
        },
        _ => Piezo::DEFAULT,
    }
}

fn pad_of(button: Button) -> Option<Pad> {
    use nibkit::input::{printed, Face};
    let face = |f| {
        Some(match printed(f) {
            Face::A => Pad::A,
            Face::B => Pad::B,
            Face::X => Pad::X,
            Face::Y => Pad::Y,
        })
    };
    match button {
        Button::A => face(Face::A),
        Button::B => face(Face::B),
        Button::X => face(Face::X),
        Button::Y => face(Face::Y),
        Button::Back => Some(Pad::Select),
        Button::Start => Some(Pad::Start),
        Button::DPadUp => Some(Pad::Up),
        Button::DPadDown => Some(Pad::Down),
        Button::DPadLeft => Some(Pad::Left),
        Button::DPadRight => Some(Pad::Right),
        Button::LeftShoulder => Some(Pad::L),
        Button::RightShoulder => Some(Pad::R),
        Button::Guide => Some(Pad::Menu),
        _ => None,
    }
}

fn nav_of(pad: Pad) -> Option<Nav> {
    Some(match pad {
        Pad::Up => Nav::Up,
        Pad::Down => Nav::Down,
        Pad::Left => Nav::Left,
        Pad::Right => Nav::Right,
        Pad::A => Nav::A,
        Pad::B => Nav::B,
        Pad::X => Nav::X,
        Pad::Y => Nav::Y,
        Pad::L => Nav::L,
        Pad::R => Nav::R,
        _ => return None,
    })
}

impl App {
    fn event(&mut self, event: Event) {
        if self.debug.input {
            match &event {
                Event::ControllerButtonDown { .. }
                | Event::ControllerButtonUp { .. }
                | Event::KeyDown { .. }
                | Event::JoyButtonDown { .. }
                | Event::ControllerDeviceAdded { .. } => eprintln!("inktick: {event:?}"),
                _ => {}
            }
        }
        match event {
            Event::Quit { .. } => self.quit = true,
            Event::KeyDown {
                keycode: Some(k),
                repeat,
                ..
            } => {
                let name = k.name();
                if let Some(cmd) = input::key(&name) {
                    self.command(Some(cmd));
                } else if let (Some(pad), false) = (input::keyboard_pad(&name), repeat) {
                    self.pad_down(pad);
                }
            }
            Event::KeyUp {
                keycode: Some(k), ..
            } => {
                if let Some(pad) = input::keyboard_pad(&k.name()) {
                    self.pad_up(pad);
                }
            }
            Event::ControllerButtonDown { button, .. } => {
                if let Some(pad) = pad_of(button) {
                    self.pad_down(pad);
                }
            }
            Event::ControllerButtonUp { button, .. } => {
                if let Some(pad) = pad_of(button) {
                    self.pad_up(pad);
                }
            }
            Event::ControllerAxisMotion { axis, value, .. } => match axis {
                Axis::LeftX => self.controls.stick_x(value),
                Axis::LeftY => self.controls.stick_y(value),
                _ => {}
            },
            Event::MouseButtonDown { x, y, .. } => {
                if let Some((fx, fy)) = self.video.bottom_point(x, y) {
                    self.touch(fx, fy, true);
                }
            }
            Event::MouseMotion {
                x, y, mousestate, ..
            } if mousestate.left() => {
                if let Some((fx, fy)) = self.video.bottom_point(x, y) {
                    self.touch(fx, fy, false);
                }
            }
            Event::MouseButtonUp { .. } => self.untouch(),
            _ => {}
        }
    }

    /// A control went down, on the gamepad or the keyboard: the menu's when it is open.
    fn pad_down(&mut self, pad: Pad) {
        if self.ui.menu {
            if let Some(nav) = nav_of(pad) {
                let action = self.ui.nav(&self.model, nav);
                self.act(action);
            }
            if pad == Pad::Menu {
                self.controls.press(pad);
            }
            return;
        }
        let cmd = self.controls.press(pad);
        if self.debug.input {
            eprintln!(
                "inktick: {pad:?} down: roles {:?}, lines {:?}",
                self.controls.held_roles(self.unit),
                self.controls.lines(self.unit)
            );
        }
        self.command(cmd);
    }

    fn pad_up(&mut self, pad: Pad) {
        let cmd = self.controls.release(pad);
        self.command(cmd);
    }

    fn command(&mut self, cmd: Option<Command>) {
        match cmd {
            Some(Command::Leave) => self.quit = true,
            Some(Command::PauseMenu) => {
                if self.ui.menu {
                    self.close_menu();
                } else {
                    self.open_menu();
                }
            }
            Some(Command::QuickSave) => self.act(Some(Action::Save(0))),
            Some(Command::QuickLoad) => self.act(Some(Action::Load(self.quick_slot))),
            None => {}
        }
    }

    fn open_menu(&mut self) {
        self.controls.clear();
        self.refresh_model();
        self.ui.open_menu();
        self.wake_bottom();
    }

    fn close_menu(&mut self) {
        self.ui.close_menu();
        self.redraw = true;
        self.video.show_lcd(Screen::Top, true);
        self.video.show_lcd(Screen::Bottom, self.stage.two_panels());
        if let Some(t) = &mut self.touch {
            t.touch_up();
        }
        // Machine time picks up from now: the pause is not caught up.
        self.origin = (Instant::now(), self.machine.ticks());
    }

    /// A finger on the bottom screen (0..1 each way): down, or moved.
    fn touch(&mut self, x: f32, y: f32, down: bool) {
        self.last_touch = Instant::now();
        if self.backlight != Backlight::Lit {
            if down {
                // The first touch only wakes the screen.
                self.wake_bottom();
            }
            return;
        }
        let (sx, sy) = ((x * ui::W as f32) as i32, (y * ui::H as f32) as i32);
        if self.ui.menu {
            let action = if down {
                self.ui.tap(&self.model, sx, sy)
            } else {
                self.ui.drag(&self.model, sx, sy)
            };
            self.act(action);
            return;
        }
        match &self.touch {
            // A single-screen unit's buttons: held while touched.
            Some(t) => {
                let role = t.hit(sx as f32, sy as f32);
                for r in self.controls.touched().to_vec() {
                    self.controls.touch_role(r, false);
                }
                if let Some(r) = role {
                    self.controls.touch_role(r, true);
                }
            }
            // A two-screen unit's lower screen: a tap opens the menu.
            None if down => self.open_menu(),
            None => {}
        }
    }

    fn untouch(&mut self) {
        for r in self.controls.touched().to_vec() {
            self.controls.touch_role(r, false);
        }
    }

    fn wake_bottom(&mut self) {
        self.last_touch = Instant::now();
        match self.backlight {
            Backlight::Dim(p) => nibkit::backlight::set(Screen::Bottom, p),
            Backlight::Off => nibkit::backlight::set_off(Screen::Bottom, false),
            Backlight::Lit => {}
        }
        self.backlight = Backlight::Lit;
    }

    fn act(&mut self, action: Option<Action>) {
        let Some(action) = action else { return };
        match action {
            Action::Save(slot) => {
                let slot = if slot == 0 { self.quick_slot } else { slot };
                self.save_slot(slot);
                self.quick_slot = slot;
                self.toast(&format!("Saved to slot {slot}"));
            }
            Action::Load(slot) => match Slot::read(&self.files.state(slot)) {
                Some(s) if self.machine.load(&s.machine) => {
                    self.quick_slot = slot.max(1);
                    self.stage.snap(self.machine.segments());
                    self.restart_clock();
                    self.toast(&format!("Slot {slot} loaded"));
                    self.close_menu();
                }
                Some(_) => self.toast("Can't load: that state is for another unit"),
                None => self.toast(&format!("Slot {slot} is empty")),
            },
            Action::Press(role) => {
                self.pressing = Some((role, Instant::now() + PRESS));
                self.close_menu();
            }
            Action::Acl => {
                self.machine.set_acl(true);
                self.acl_until = Some(Instant::now() + PRESS);
                self.close_menu();
            }
            Action::Set {
                key,
                value,
                for_game,
            } => {
                self.options.set(&key, value, for_game);
                self.apply_options(&key);
            }
            Action::Unset { key, for_game } => {
                self.options.unset(&key, for_game);
                self.apply_options(&key);
            }
            Action::OpenMenu => self.open_menu(),
            Action::Clock(hour12) => {
                if let Err(e) = nibkit::clock::set_hour12(hour12) {
                    eprintln!("inktick: cannot keep the clock setting: {e}");
                }
                self.model.clock = clock_now();
            }
            Action::Brightness { top, percent } => {
                let screen = if top { Screen::Top } else { Screen::Bottom };
                nibkit::backlight::set(screen, percent);
                nibkit::backlight::save();
            }
            Action::Volume(p) => nibkit::device::set_volume(p),
            Action::Resume => self.close_menu(),
            Action::Quit => self.quit = true,
        }
        self.refresh_model();
    }

    /// Hands the stage's LCDs to the GPU: the top panel's, and the bottom's for a two-screen
    /// unit (a single-screen unit's bottom panel is its buttons, uploaded as a picture).
    fn give_lcds(&mut self) -> Result<(), String> {
        self.video
            .set_lcd(Screen::Top, Some(&self.stage.panels[0].lcd()))?;
        let bottom = self.stage.panels.get(1).map(|p| p.lcd());
        self.video.set_lcd(Screen::Bottom, bottom.as_ref())?;
        self.redraw = true;
        Ok(())
    }

    /// An option changed: what it changes, now.
    fn apply_options(&mut self, key: &str) {
        match key {
            "view" | "ghosting" | "shadow" | "ink" => {
                let view = ViewChoice::parse(&self.options.str("view"));
                self.stage = Stage::new(&self.set, view, look_of(&self.options));
                self.stage.snap(self.machine.segments());
                self.touch =
                    (!self.stage.two_panels()).then(|| ui::controls::Controls::new(self.unit));
                if let Err(e) = self.give_lcds() {
                    eprintln!("inktick: {e}");
                }
                self.redraw = true;
            }
            "sound" => self.synth.set_piezo(piezo_of(&self.options)),
            "clock" => self.restart_clock(),
            _ => {}
        }
    }

    /// Machine time starts following real time from now, and the unit's clock is set from the
    /// system's.
    fn restart_clock(&mut self) {
        self.origin = (Instant::now(), self.machine.ticks());
        self.restart_sound();
        self.sync_clock();
    }

    /// Sound starts from the machine's time now: none is made for time already past (after a
    /// load, a resume or a stall it would only sit in the queue as a delay).
    fn restart_sound(&mut self) {
        self.machine.take_edges();
        self.synth.restart(self.machine.ticks());
        if let Some(q) = &self.audio {
            q.clear();
        }
    }

    /// Sets the unit's clock to the local time, where the options and the unit's database entry
    /// allow: on every start, load, ACL and wake.
    fn sync_clock(&mut self) {
        if self.options.str("clock") == "system" {
            set_clock(&mut self.machine, self.unit);
        }
    }

    fn toast(&mut self, text: &str) {
        self.model.toast = Some(text.to_owned());
        self.toast_until = Some(Instant::now() + Duration::from_secs(2));
        self.ui.touch_up();
    }

    fn played(&self) -> u64 {
        self.played_before + self.started.elapsed().as_secs()
    }

    fn save_slot(&mut self, slot: usize) {
        let machine = self.machine.save();
        self.stage.refresh_pictures();
        let thumb = self.stage.thumbnail();
        let s = Slot {
            made: saves::unix_now(),
            played: self.played(),
            png: thumb.to_png(),
            machine,
        };
        self.writer.write(self.files.state(slot), s.encode());
        self.model.slots.resize(SLOTS + 1, SlotInfo::default());
        self.model.slots[slot] = SlotInfo {
            picture: Some((thumb.pixels, thumb.width, thumb.height)),
            made: when(s.made),
            played: played(s.played),
        };
    }

    /// Leaving: the resume point, the screens lit again.
    fn leave(&mut self) {
        self.save_slot(0);
        self.wake_bottom();
        self.writer.finish();
    }

    /// INKTICK_DEBUG's scripted presses, screenshots and leaving.
    fn debug_step(&mut self) {
        let t = self.began.elapsed().as_secs_f64();
        let due: Vec<Role> = self
            .debug
            .presses
            .iter()
            .filter(|(_, at)| *at <= t)
            .map(|(r, _)| *r)
            .collect();
        self.debug.presses.retain(|(_, at)| *at > t);
        for role in due {
            self.pressing = Some((role, Instant::now() + PRESS));
        }
        if !self.debug.done && t >= self.debug.at {
            self.debug.done = true;
            if let Some(page) = self.debug.menu.clone() {
                self.open_menu();
                self.ui.go_named(&page);
            }
            if let Some(dir) = self.debug.shot.clone() {
                let _ = std::fs::create_dir_all(&dir);
                self.stage.refresh_pictures();
                let top = self.stage.panels[0].picture().clone();
                let _ = std::fs::write(dir.join("top.png"), top.to_png());
                if let Some(px) = self.video.read_lcd(Screen::Top) {
                    let img = inktick_art::Image {
                        width: 1024,
                        height: 768,
                        pixels: px,
                    };
                    let _ = std::fs::write(dir.join("top-gpu.png"), img.to_png());
                }
                if let Some(px) = self.video.read_window(Screen::Top) {
                    let img = inktick_art::Image {
                        width: 1024,
                        height: 768,
                        pixels: px,
                    };
                    let _ = std::fs::write(dir.join("top-window.png"), img.to_png());
                }
                let bottom = if self.ui.menu {
                    self.ui.touch_up();
                    self.ui.render(&self.model).map(<[u8]>::to_vec)
                } else if let Some(t) = &mut self.touch {
                    t.touch_up();
                    t.render().map(<[u8]>::to_vec)
                } else {
                    self.stage.panels.get(1).map(|p| p.picture().pixels.clone())
                };
                if let Some(px) = bottom {
                    let img = inktick_art::Image {
                        width: 1024,
                        height: 768,
                        pixels: px,
                    };
                    let _ = std::fs::write(dir.join("bottom.png"), img.to_png());
                }
                eprintln!("inktick: screens saved in {}", dir.display());
            }
        }
        if self.debug.quit.is_some_and(|q| t >= q) {
            self.quit = true;
        }
    }

    fn frame(&mut self) {
        self.debug_step();
        self.last_frame = Instant::now();
        if !self.ui.menu {
            let mut lines = self.controls.lines(self.unit);
            if let Some((role, until)) = self.pressing {
                if Instant::now() < until {
                    let pressed = self.unit.lines(|b| b.role == role);
                    for (l, p) in lines.matrix.iter_mut().zip(pressed.matrix) {
                        *l |= p;
                    }
                    lines.ba &= pressed.ba;
                    lines.beta &= pressed.beta;
                } else {
                    self.pressing = None;
                }
            }
            if self.acl_until.is_some_and(|t| Instant::now() >= t) {
                self.acl_until = None;
                self.machine.set_acl(false);
                // Kept to the system's time, the unit goes back to its clock, set; on its own
                // time it shows every segment until TIME, as the unit does.
                if self.options.str("clock") == "system" {
                    self.unit.show_clock(&mut self.machine);
                    self.restart_clock();
                }
            }
            self.machine.set_lines(lines);
            // Machine time follows real time; a stall (a suspend) is not caught up.
            let (at, tick) = self.origin;
            let mut due = tick + (at.elapsed().as_secs_f64() * CRYSTAL_HZ as f64) as u64;
            if due > self.machine.ticks() + CRYSTAL_HZ as u64 / 4 {
                self.origin = (Instant::now(), self.machine.ticks());
                due = self.machine.ticks() + CRYSTAL_HZ as u64 / 60;
                self.restart_sound();
                self.sync_clock();
            }
            let t0 = Instant::now();
            self.machine.run_until(due);
            self.spent[0] += t0.elapsed();
            let t5 = Instant::now();
            self.sound();
            self.spent[5] += t5.elapsed();
            if let Some(t) = &mut self.touch {
                t.set_held(&self.controls.held_roles(self.unit));
            }
        }
        // The LCD follows the unit's segments at its own pace, also while paused.
        let t1 = Instant::now();
        // A segment switching shows at the next refresh; a fade alone moves on every other one
        // (30 a second is smooth for a fade, and halves the drawing).
        let switched = *self.machine.segments() != self.shown;
        self.odd = !self.odd;
        let changed = (switched || self.odd)
            && self.stage.update(
                self.machine.segments(),
                self.since_optics.elapsed().as_secs_f32() * 1000.0,
            );
        if switched || self.odd {
            self.since_optics = Instant::now();
            self.shown = *self.machine.segments();
        }
        self.spent[1] += t1.elapsed();
        let t2 = Instant::now();
        let upload_before = self.spent[3];
        let mut top = false;
        let mut bottom = false;
        if (changed || self.redraw) && !self.ui.menu {
            // The GPU draws the LCDs from the optics; the CPU does no pixel work.
            let u = Instant::now();
            let optics = &self.stage.optics;
            let level = |s: usize, i: usize| {
                optics
                    .get(s)
                    .and_then(|o| o.level.get(i))
                    .copied()
                    .unwrap_or(0.0)
            };
            if self.debug.skip != "lcd" {
                self.video.draw_lcd(Screen::Top, level);
            }
            top = true;
            if self.stage.two_panels() {
                self.video.draw_lcd(Screen::Bottom, level);
                bottom = true;
            }
            self.spent[3] += u.elapsed();
            self.redraw = false;
        }
        self.spent[2] += t2
            .elapsed()
            .saturating_sub(self.spent[3].saturating_sub(upload_before));
        // The menu: over the top screen, and on the bottom.
        if let Some(overlay) = self.ui.render_top(&self.model) {
            // The paused LCD under the menu's overlay: composed on the CPU, once.
            self.stage.refresh_pictures();
            let mut px = self.stage.panels[0].picture().pixels.clone();
            if let Some(o) = overlay {
                for (p, q) in px.chunks_exact_mut(4).zip(o.chunks_exact(4)) {
                    let a = q[3] as u32;
                    for c in 0..3 {
                        p[c] = (q[c] as u32 + p[c] as u32 * (255 - a) / 255).min(255) as u8;
                    }
                }
            }
            self.video.upload(Screen::Top, &px, &[full_rect()]);
            self.video
                .show_lcd(Screen::Top, overlay.is_none() && !self.ui.menu);
            top = true;
        }
        if self.ui.menu {
            if let Some(px) = self.ui.render(&self.model) {
                let px = px.to_vec();
                self.video.upload(Screen::Bottom, &px, &[full_rect()]);
                self.video.show_lcd(Screen::Bottom, false);
                bottom = true;
            }
        } else if let Some(t) = &mut self.touch {
            if let Some(px) = t.render() {
                let px = px.to_vec();
                self.video.upload(Screen::Bottom, &px, &[full_rect()]);
                bottom = true;
            }
        }
        if top || bottom {
            let t4 = Instant::now();
            if self.debug.skip != "present" {
                self.video.present(top, bottom);
            }
            self.spent[4] += t4.elapsed();
            self.presented += 1;
        }
        if self.debug.stats && self.since.elapsed() >= Duration::from_secs(5) {
            let secs = self.since.elapsed().as_secs_f64();
            let ms = |d: Duration| d.as_secs_f64() * 1000.0 / secs;
            eprintln!(
                "inktick: per second: unit {:.2} ms, sound {:.2} ms, optics {:.2} ms, compose {:.2} ms, upload {:.2} ms, present {:.2} ms; {:.1} frames shown",
                ms(self.spent[0]),
                ms(self.spent[5]),
                ms(self.spent[1]),
                ms(self.spent[2]),
                ms(self.spent[3]),
                ms(self.spent[4]),
                self.presented as f64 / secs
            );
            self.spent = [Duration::ZERO; 6];
            self.presented = 0;
            self.since = Instant::now();
        }
        if self.toast_until.is_some_and(|t| Instant::now() > t) {
            self.toast_until = None;
            self.model.toast = None;
            self.ui.touch_up();
        }
        self.idle_bottom();
        let clock = clock_now();
        if clock != self.model.clock && self.ui.menu {
            self.model.clock = clock;
            self.model.battery = battery();
            self.ui.touch_up();
        }
    }

    /// The buzzer's sound so far, into the queue.
    fn sound(&mut self) {
        if self.audio.is_none() {
            self.machine.take_edges();
            return;
        }
        self.machine.sound_into(&mut self.synth);
        let n = self.synth.ready(self.machine.ticks());
        self.samples.clear();
        self.synth.take(n, &mut self.samples);
        let Some(q) = &self.audio else { return };
        let queued = q.size() / 2;
        if queued > AUDIO_MOST {
            return;
        }
        self.pcm.clear();
        self.pcm.extend(
            self.samples
                .iter()
                .map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16),
        );
        if self.debug.quiet {
            // Measured, not heard: the loudest sample, logged with the stats.
            let peak = self.pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
            self.loudest = self.loudest.max(peak);
            self.pcm.iter_mut().for_each(|s| *s = 0);
        }
        let _ = q.queue_audio(&self.pcm);
    }

    fn idle_bottom(&mut self) {
        // A two-screen unit's lower screen is part of the game: it stays lit.
        if self.ui.menu
            || self.stage.two_panels()
            || self.backlight != Backlight::Lit
            || self.last_touch.elapsed() < IDLE
        {
            return;
        }
        match self.options.str("bottom").as_str() {
            "off" => {
                nibkit::backlight::set_off(Screen::Bottom, true);
                self.backlight = Backlight::Off;
            }
            "dim" => {
                if let Some(p) = nibkit::backlight::get(Screen::Bottom) {
                    nibkit::backlight::set(Screen::Bottom, 0);
                    self.backlight = Backlight::Dim(p);
                }
            }
            _ => {}
        }
    }

    fn refresh_model(&mut self) {
        let m = &mut self.model;
        m.title = self.unit.title.to_owned();
        m.console = CONSOLE.to_owned();
        m.quick_slot = self.quick_slot;
        m.clock = clock_now();
        m.battery = battery();
        m.top_brightness = nibkit::backlight::get(Screen::Top);
        m.bottom_brightness = match self.backlight {
            Backlight::Dim(p) => Some(p),
            _ => nibkit::backlight::get(Screen::Bottom),
        };
        m.volume = nibkit::device::volume();
        if m.slots.len() != SLOTS + 1 {
            m.slots = (0..=SLOTS)
                .map(|i| slot_info(&self.files.state(i)))
                .collect();
        }
        let secs = self.started.elapsed().as_secs();
        m.played = if secs >= 60 {
            played(secs).replace(" played", " today")
        } else {
            String::new()
        };
        m.hour12 = hour12();
        // The unit's own buttons are on Play where the bottom screen is the lower LCD.
        m.unit_buttons = if self.stage.two_panels() {
            self.unit
                .buttons
                .iter()
                .filter(|b| b.role.is_unit_button())
                .map(|b| (b.label.to_owned(), b.role))
                .collect()
        } else {
            Vec::new()
        };
        m.game_options = option_rows(&self.options, true);
        m.console_options = option_rows(&self.options, false);
        self.ui.touch_up();
    }
}

fn full_rect() -> inktick_art::Rect {
    inktick_art::Rect {
        x: 0,
        y: 0,
        w: nibkit::screens::WIDTH as i32,
        h: nibkit::screens::HEIGHT as i32,
    }
}

/// Sets a unit's clock to the local time now, through its database entry.
fn set_clock(machine: &mut Machine, unit: &Unit) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let tm = local(now);
    unit.set_time(machine, tm.tm_hour as u8, tm.tm_min as u8, tm.tm_sec as u8);
}

/// The options as their rows show them at one level: the unit's (`for_game`) or every unit's.
fn option_rows(o: &Options, for_game: bool) -> Vec<OptionRow> {
    o.keys()
        .into_iter()
        .map(|key| {
            let here = o.here(key, for_game);
            let (value, from) = o.inherited(key, for_game);
            let values: Vec<_> = std::iter::once(None)
                .chain(o.values(key).into_iter().map(|(v, _)| Some(v)))
                .collect();
            let at = here
                .as_ref()
                .and_then(|h| values.iter().position(|v| v.as_ref() == Some(h)))
                .unwrap_or(0);
            OptionRow {
                key: key.to_owned(),
                label: o.label(key),
                here: here.as_ref().map(|h| o.show(key, h)),
                inherit: format!("Use {from} ({})", o.show(key, &value)),
                values,
                at,
                wraps: o.wraps(key),
                use_hint: format!("use {from}"),
                shows: matches!(key, "view" | "ghosting" | "shadow" | "ink"),
            }
        })
        .collect()
}

/// Without a set on the command line: the list of units on the cards, until one is chosen
/// (`None`: the player left, or SIGTERM).
fn pick(
    video: &mut Video,
    events: &mut sdl2::EventPump,
    touch: &mut Option<nibkit::touch::Panel>,
) -> Option<PathBuf> {
    let mut picker = picker::Picker::new(picker::find(&nibkit::paths::rom_roots()));
    video.upload(Screen::Top, &vec![0u8; 1024 * 768 * 4], &[full_rect()]);
    let mut pointers = Vec::new();
    loop {
        if TERM.load(Ordering::Relaxed) {
            return None;
        }
        let mut start = None;
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => return None,
                Event::KeyDown {
                    keycode: Some(k), ..
                } => {
                    let name = k.name();
                    if nibkit::input::BACK_KEYCODES.contains(&name.as_str()) || name == "Escape" {
                        return None;
                    }
                    let key = match input::keyboard_pad(&name) {
                        Some(Pad::Up) => nibkit::menu::Key::Up,
                        Some(Pad::Down) => nibkit::menu::Key::Down,
                        Some(Pad::A | Pad::Start) => nibkit::menu::Key::A,
                        _ => continue,
                    };
                    match picker.key(key) {
                        Ok(chosen) => start = chosen.or(start),
                        Err(()) => return None,
                    }
                }
                Event::ControllerButtonDown { button, .. } => {
                    use nibkit::menu::Key;
                    let key = match pad_of(button) {
                        Some(Pad::Up) => Key::Up,
                        Some(Pad::Down) => Key::Down,
                        Some(Pad::L | Pad::Left) => Key::L,
                        Some(Pad::R | Pad::Right) => Key::R,
                        Some(Pad::A | Pad::Start) => Key::A,
                        Some(Pad::B) => Key::B,
                        _ => continue,
                    };
                    match picker.key(key) {
                        Ok(chosen) => start = chosen.or(start),
                        Err(()) => return None,
                    }
                }
                Event::MouseButtonDown { x, y, .. } => {
                    start = video
                        .bottom_point(x, y)
                        .and_then(|(fx, fy)| picker.touch(fx, fy));
                }
                _ => {}
            }
        }
        if let Some(panel) = touch {
            pointers.clear();
            panel.poll(&mut pointers);
            for p in pointers.drain(..) {
                if let nibkit::touch::Pointer::Down(x, y) = p {
                    start = picker.touch(x, y).or(start);
                }
            }
        }
        if let Some(game) = start.and_then(|i| picker.games.get(i)) {
            return Some(game.path.clone());
        }
        if let Some(px) = picker.render(&clock_now()) {
            let px = px.to_vec();
            video.upload(Screen::Bottom, &px, &[full_rect()]);
            video.present(true, true);
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

fn decode_png(data: &[u8]) -> Option<(Vec<u8>, usize, usize)> {
    let img = inktick_art::Image::from_png(data).ok()?;
    Some((img.pixels, img.width, img.height))
}

fn slot_info(path: &Path) -> SlotInfo {
    match Slot::read(path) {
        Some(s) => SlotInfo {
            picture: decode_png(&s.png),
            made: when(s.made),
            played: played(s.played),
        },
        None => SlotInfo::default(),
    }
}

/// A Unix time as "Oct 19 12:40", in local time.
fn when(t: u64) -> String {
    let tm = local(t);
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{} {} {}",
        MONTHS[tm.tm_mon.clamp(0, 11) as usize],
        tm.tm_mday,
        nibkit::clock::format(tm.tm_hour as u32, tm.tm_min as u32, hour12())
    )
}

/// A Unix time in local time.
fn local(t: u64) -> libc::tm {
    let secs = t as libc::time_t;
    // SAFETY: localtime_r fills the struct given.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    }
}

/// Whether times show as 12-hour: the player's choice, shared with DSlot and the rest of nibOS
/// (`nibkit::clock`), read again each second so a change in DSlot shows at once.
fn hour12() -> bool {
    use std::sync::Mutex;
    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    match *cache {
        Some((at, value)) if at.elapsed() < Duration::from_secs(1) => value,
        _ => {
            let value = nibkit::clock::hour12();
            *cache = Some((Instant::now(), value));
            value
        }
    }
}

fn played(secs: u64) -> String {
    let (h, m) = (secs / 3600, secs / 60 % 60);
    if h > 0 {
        format!("{h} h {m} min played")
    } else {
        format!("{m} min played")
    }
}

fn clock_now() -> String {
    let t = when(saves::unix_now());
    t.splitn(3, ' ').nth(2).unwrap_or("").to_owned()
}

/// The battery's charge, from the first power supply that is a battery.
fn battery() -> Option<u8> {
    let dir = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for e in dir.flatten() {
        let p = e.path();
        if std::fs::read_to_string(p.join("type")).is_ok_and(|t| t.trim() == "Battery") {
            return std::fs::read_to_string(p.join("capacity"))
                .ok()?
                .trim()
                .parse()
                .ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_unit_has_a_title_of_its_own_in_dslots_list() {
        let list = super::units_list();
        let titles: Vec<&str> = list
            .lines()
            .map(|l| l.split('\t').nth(1).unwrap())
            .collect();
        let mut unique = titles.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), titles.len(), "{titles:?}");
        assert!(titles.contains(&"Donkey Kong Jr. (Panorama)"));
        assert!(titles.contains(&"Zelda"));
    }

    use super::*;

    #[test]
    fn played_time() {
        assert_eq!(played(59), "0 min played");
        assert_eq!(played(3600 + 23 * 60), "1 h 23 min played");
    }
}
