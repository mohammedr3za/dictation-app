//! Modifier-only shortcuts such as Option+Command.
//!
//! The global shortcut plugin registers through the OS hotkey APIs, which need a
//! regular key. A chord made only of modifiers is detected here instead by
//! watching the keyboard directly: an event tap on macOS, and polling the key
//! state over X11 on Linux. Wayland allows neither, so it is unsupported there.

use std::sync::{Arc, Mutex};

pub const CONTROL: u8 = 1;
pub const ALT: u8 = 2;
pub const SHIFT: u8 = 4;
pub const COMMAND: u8 = 8;

/// Parses a modifier-only shortcut such as "Alt+Command" into a modifier mask.
/// Returns None for anything else, including a single modifier, which would
/// fire during ordinary typing.
pub fn parse_chord(accelerator: &str) -> Option<u8> {
    let mut mask = 0;
    for part in accelerator.split('+') {
        mask |= match part.trim() {
            "Control" => CONTROL,
            "Alt" => ALT,
            "Shift" => SHIFT,
            "Command" => COMMAND,
            _ => return None,
        };
    }
    (mask.count_ones() >= 2).then_some(mask)
}

/// Recognizes a tap of a modifier chord: every modifier in the chord held
/// together, then all released, with no other key, click, or extra modifier in
/// between. Anything in between means the user was pressing some other shortcut
/// (Option+Command+Esc, say), so it does not count.
#[derive(Debug)]
pub struct TapRecognizer {
    chord: u8,
    held: u8,
    armed: bool,
    spoiled: bool,
}

impl TapRecognizer {
    pub fn new(chord: u8) -> Self {
        Self {
            chord,
            held: 0,
            armed: false,
            spoiled: false,
        }
    }

    pub fn chord(&self) -> u8 {
        self.chord
    }

    /// Reports the modifiers now held. Returns true when this completes a tap.
    pub fn modifiers(&mut self, held: u8) -> bool {
        self.held = held;
        if held == 0 {
            let tapped = self.armed && !self.spoiled;
            self.armed = false;
            self.spoiled = false;
            return tapped;
        }
        if held & !self.chord != 0 {
            self.spoiled = true;
        }
        if held == self.chord {
            self.armed = true;
        }
        false
    }

    /// Reports a non-modifier key press or a mouse click.
    pub fn other_input(&mut self) {
        if self.held != 0 {
            self.spoiled = true;
        }
    }
}

struct Shared {
    /// The chord being watched; None while no modifier-only shortcut is set.
    chord: Mutex<Option<u8>>,
    /// Whether the platform listener thread is running.
    listening: Mutex<bool>,
    on_tap: Box<dyn Fn() + Send + Sync>,
}

impl Shared {
    fn chord(&self) -> Option<u8> {
        self.chord.lock().ok().and_then(|chord| *chord)
    }

    /// Returns the recognizer for the chord currently watched, replacing it when
    /// the chord changed, or None while nothing is watched.
    fn recognizer<'a>(&self, slot: &'a mut Option<TapRecognizer>) -> Option<&'a mut TapRecognizer> {
        match self.chord() {
            None => {
                *slot = None;
                None
            }
            Some(chord) => {
                if slot.as_ref().map(TapRecognizer::chord) != Some(chord) {
                    *slot = Some(TapRecognizer::new(chord));
                }
                slot.as_mut()
            }
        }
    }
}

/// Watches for taps of one modifier chord and calls the handler on each.
pub struct ModifierMonitor {
    shared: Arc<Shared>,
}

impl ModifierMonitor {
    pub fn new(on_tap: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            shared: Arc::new(Shared {
                chord: Mutex::new(None),
                listening: Mutex::new(false),
                on_tap: Box::new(on_tap),
            }),
        }
    }

    /// Starts reporting taps of `chord`, or stops when it is None. The listener
    /// thread starts on first use and is kept; stopping only mutes it.
    pub fn watch(&self, chord: Option<u8>) -> Result<(), String> {
        if let Ok(mut current) = self.shared.chord.lock() {
            *current = chord;
        }
        if chord.is_none() {
            return Ok(());
        }
        let mut listening = self
            .shared
            .listening
            .lock()
            .map_err(|_| "shortcut state poisoned".to_string())?;
        if !*listening {
            platform::start(Arc::clone(&self.shared))?;
            *listening = true;
        }
        Ok(())
    }
}

/// Prints a line for every tap of `accelerator`, to check a shortcut from a
/// terminal without the app window.
pub fn watch_cli(accelerator: &str) -> i32 {
    use std::io::Write;

    let Some(chord) = parse_chord(accelerator) else {
        eprintln!("not a modifier-only shortcut: {accelerator}");
        return 2;
    };
    let monitor = ModifierMonitor::new(|| {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "tap");
        let _ = out.flush();
    });
    if let Err(error) = monitor.watch(Some(chord)) {
        eprintln!("error: {error}");
        return 1;
    }
    eprintln!("watching {accelerator}; press Ctrl+C to stop");
    loop {
        std::thread::park();
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use core_foundation::runloop::{kCFRunLoopCommonModes, kCFRunLoopDefaultMode, CFRunLoop};
    use core_graphics::event::{
        CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
        CGEventType, CallbackResult,
    };
    use std::cell::RefCell;
    use std::sync::mpsc;
    use std::time::Duration;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightListenEventAccess() -> bool;
        fn CGRequestListenEventAccess() -> bool;
    }

    const PERMISSION: &str = "Allow Dictation in System Settings → Privacy & Security → \
        Input Monitoring, then reopen the app. Modifier-only shortcuts need it.";

    pub fn start(shared: Arc<Shared>) -> Result<(), String> {
        // SAFETY: argument-free CoreGraphics permission queries.
        if !unsafe { CGPreflightListenEventAccess() } {
            // Shows the system prompt the first time; later calls do nothing.
            unsafe { CGRequestListenEventAccess() };
            return Err(PERMISSION.into());
        }
        let (ready, started) = mpsc::channel();
        std::thread::Builder::new()
            .name("modifier-tap".into())
            .spawn(move || run(shared, ready))
            .map_err(|e| e.to_string())?;
        started
            .recv()
            .map_err(|_| "keyboard listener exited".to_string())?
    }

    fn mask(flags: CGEventFlags) -> u8 {
        let mut mask = 0;
        if flags.contains(CGEventFlags::CGEventFlagControl) {
            mask |= CONTROL;
        }
        if flags.contains(CGEventFlags::CGEventFlagAlternate) {
            mask |= ALT;
        }
        if flags.contains(CGEventFlags::CGEventFlagShift) {
            mask |= SHIFT;
        }
        if flags.contains(CGEventFlags::CGEventFlagCommand) {
            mask |= COMMAND;
        }
        mask
    }

    fn run(shared: Arc<Shared>, ready: mpsc::Sender<Result<(), String>>) {
        let recognizer = RefCell::new(None);
        let callback_shared = Arc::clone(&shared);
        let tap = CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![
                CGEventType::FlagsChanged,
                CGEventType::KeyDown,
                CGEventType::LeftMouseDown,
                CGEventType::RightMouseDown,
                CGEventType::OtherMouseDown,
            ],
            move |_, event_type, event| {
                let mut slot = recognizer.borrow_mut();
                if let Some(recognizer) = callback_shared.recognizer(&mut slot) {
                    match event_type {
                        CGEventType::FlagsChanged => {
                            if recognizer.modifiers(mask(event.get_flags())) {
                                (callback_shared.on_tap)();
                            }
                        }
                        CGEventType::KeyDown
                        | CGEventType::LeftMouseDown
                        | CGEventType::RightMouseDown
                        | CGEventType::OtherMouseDown => recognizer.other_input(),
                        _ => {}
                    }
                }
                CallbackResult::Keep
            },
        );

        let Ok(tap) = tap else {
            let _ = ready.send(Err("could not watch the keyboard".into()));
            return;
        };
        let Ok(source) = tap.mach_port().create_runloop_source(0) else {
            let _ = ready.send(Err("could not watch the keyboard".into()));
            return;
        };
        // SAFETY: reading CoreFoundation's run loop mode constants.
        CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
        tap.enable();
        let _ = ready.send(Ok(()));

        loop {
            CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, Duration::from_secs(2), false);
            // macOS disables a tap it considers slow, and during secure text
            // entry. Enabling an enabled tap does nothing.
            tap.enable();
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt, KeyButMask};

    pub fn start(shared: Arc<Shared>) -> Result<(), String> {
        if std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind == "wayland") {
            return Err("Modifier-only shortcuts need an X11 session. Wayland does not let \
                apps watch the keyboard; pick a shortcut with a key instead."
                .into());
        }
        let (ready, started) = mpsc::channel();
        std::thread::Builder::new()
            .name("modifier-poll".into())
            .spawn(move || {
                if let Err(error) = poll(&shared, &ready) {
                    // Only reaches the caller if polling never started.
                    let _ = ready.send(Err(error));
                }
            })
            .map_err(|e| e.to_string())?;
        started
            .recv()
            .map_err(|_| "keyboard listener exited".to_string())?
    }

    fn poll(shared: &Shared, ready: &mpsc::Sender<Result<(), String>>) -> Result<(), String> {
        let x11 = |e: &dyn std::fmt::Display| format!("X11: {e}");
        let (conn, screen) = x11rb::connect(None).map_err(|e| x11(&e))?;
        let root = conn.setup().roots[screen].root;

        // X has eight modifier slots: Shift, Lock, Control, Mod1 (Alt), then
        // Mod2–Mod5, where Mod4 is Super. Keycodes in any slot are modifiers and
        // never count as "another key".
        let mapping = conn
            .get_modifier_mapping()
            .map_err(|e| x11(&e))?
            .reply()
            .map_err(|e| x11(&e))?;
        let per_slot = (mapping.keycodes.len() / 8).max(1);
        let mut bits = [0u8; 256];
        let mut is_modifier = [false; 256];
        for (slot, codes) in mapping.keycodes.chunks(per_slot).enumerate() {
            for &code in codes.iter().filter(|&&code| code != 0) {
                is_modifier[code as usize] = true;
                bits[code as usize] |= match slot {
                    0 => SHIFT,
                    2 => CONTROL,
                    3 => ALT,
                    6 => COMMAND,
                    _ => 0,
                };
            }
        }
        let buttons = u16::from(KeyButMask::BUTTON1 | KeyButMask::BUTTON2 | KeyButMask::BUTTON3);
        let _ = ready.send(Ok(()));

        let mut slot = None;
        loop {
            let Some(recognizer) = shared.recognizer(&mut slot) else {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            };

            let keys = conn
                .query_keymap()
                .map_err(|e| x11(&e))?
                .reply()
                .map_err(|e| x11(&e))?
                .keys;
            let mut held = 0;
            let mut other = false;
            for code in 0..256 {
                if keys[code / 8] & (1 << (code % 8)) != 0 {
                    if is_modifier[code] {
                        held |= bits[code];
                    } else {
                        other = true;
                    }
                }
            }
            let pointer = conn
                .query_pointer(root)
                .map_err(|e| x11(&e))?
                .reply()
                .map_err(|e| x11(&e))?;
            if u16::from(pointer.mask) & buttons != 0 {
                other = true;
            }

            if recognizer.modifiers(held) {
                (shared.on_tap)();
            }
            if other {
                recognizer.other_input();
            }
            // Fast enough to catch a quick tap, which lasts well over 50 ms.
            std::thread::sleep(Duration::from_millis(15));
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod platform {
    use super::*;

    pub fn start(_shared: Arc<Shared>) -> Result<(), String> {
        Err("Modifier-only shortcuts are not supported on this platform.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPT_CMD: u8 = ALT | COMMAND;

    #[test]
    fn parses_modifier_chords() {
        assert_eq!(parse_chord("Alt+Command"), Some(OPT_CMD));
        assert_eq!(parse_chord("Control+Shift+Alt"), Some(CONTROL | SHIFT | ALT));
        assert_eq!(parse_chord("Command"), None, "one modifier fires while typing");
        assert_eq!(parse_chord("Alt+Alt"), None);
        assert_eq!(parse_chord("Alt+Command+KeyD"), None, "has a main key");
        assert_eq!(parse_chord(""), None);
    }

    #[test]
    fn tap_fires_on_release() {
        let mut r = TapRecognizer::new(OPT_CMD);
        assert!(!r.modifiers(ALT));
        assert!(!r.modifiers(OPT_CMD));
        assert!(!r.modifiers(COMMAND), "fires only when everything is released");
        assert!(r.modifiers(0));
    }

    #[test]
    fn either_press_order_works() {
        let mut r = TapRecognizer::new(OPT_CMD);
        r.modifiers(COMMAND);
        r.modifiers(OPT_CMD);
        r.modifiers(ALT);
        assert!(r.modifiers(0));
    }

    #[test]
    fn other_key_cancels() {
        // Option+Command+Esc must not toggle dictation.
        let mut r = TapRecognizer::new(OPT_CMD);
        r.modifiers(OPT_CMD);
        r.other_input();
        assert!(!r.modifiers(0));
    }

    #[test]
    fn extra_modifier_cancels() {
        let mut r = TapRecognizer::new(OPT_CMD);
        r.modifiers(OPT_CMD);
        r.modifiers(OPT_CMD | SHIFT);
        r.modifiers(OPT_CMD);
        assert!(!r.modifiers(0));
    }

    #[test]
    fn extra_modifier_before_the_chord_cancels() {
        let mut r = TapRecognizer::new(OPT_CMD);
        r.modifiers(SHIFT);
        r.modifiers(SHIFT | OPT_CMD);
        r.modifiers(OPT_CMD);
        assert!(!r.modifiers(0));
    }

    #[test]
    fn partial_chord_does_not_fire() {
        let mut r = TapRecognizer::new(OPT_CMD);
        r.modifiers(ALT);
        assert!(!r.modifiers(0));
        r.modifiers(COMMAND);
        assert!(!r.modifiers(0));
    }

    #[test]
    fn typing_before_the_chord_does_not_cancel_it() {
        let mut r = TapRecognizer::new(OPT_CMD);
        r.other_input();
        r.modifiers(OPT_CMD);
        assert!(r.modifiers(0));
    }

    #[test]
    fn recovers_after_a_cancelled_tap() {
        let mut r = TapRecognizer::new(OPT_CMD);
        r.modifiers(OPT_CMD);
        r.other_input();
        assert!(!r.modifiers(0));
        r.modifiers(OPT_CMD);
        assert!(r.modifiers(0));
    }

    #[test]
    fn synthetic_paste_does_not_fire() {
        // The app pastes with Command+V; that must never toggle Option+Command.
        let mut r = TapRecognizer::new(OPT_CMD);
        r.modifiers(COMMAND);
        r.other_input();
        assert!(!r.modifiers(0));
    }
}
