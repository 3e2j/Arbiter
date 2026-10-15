//! Opens the OS window, draws into it, and turns its events into
//! [`Event`]s.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use super::pacer::Pacer;
use crate::{
    canvas::{Canvas, Rect},
    cast::narrow,
    host::{App, Host, Wake},
    input::{CursorIcon, Event, Key, KeyCode, Modifiers, PointerButton, WindowAction, WindowEdge},
    render::{self, Gpu},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{
        DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent,
    },
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{self, NamedKey, PhysicalKey},
    window::{CursorGrabMode, ResizeDirection, Window, WindowId},
};

/// Used when the monitor doesn't report its refresh rate.
const FALLBACK_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);
/// How far a wheel's line of scrolling goes, in logical pixels.
const SCROLL_LINE: f32 = 40.;
/// How soon and how near, in logical pixels, a press follows the last of the
/// same button to make a double click. Windows' defaults, as winit doesn't
/// read the system's.
const DOUBLE_CLICK_TIME: Duration = Duration::from_millis(500);
const DOUBLE_CLICK_DISTANCE: f32 = 4.;

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    EventLoop(#[from] winit::error::EventLoopError),

    #[error("couldn't open a window: {0}")]
    Os(#[from] winit::error::OsError),

    #[error(transparent)]
    Gpu(#[from] render::Error),

    // `Box<dyn Error>` doesn't implement `Error`, so it can't be a source.
    #[error("{0}")]
    App(Box<dyn std::error::Error>),
}

/// Opens the window and blocks until it's closed.
///
/// # Errors
///
/// When the app can't start, or the window or its GPU surface can't be opened,
/// including again after the surface is lost.
pub fn run<A: App>(args: A::Args) -> Result<(), RunError> {
    let event_loop = EventLoop::new()?;
    let proxy = event_loop.create_proxy();
    // Fails only once the loop has closed, when there's nothing to wake.
    let wake = Wake::new(move || {
        let _ = proxy.send_event(());
    });
    let mut runner = Runner {
        host: Host::<A>::new(args, wake).map_err(RunError::App)?,
        canvas: Canvas::default(),
        open: None,
        error: None,
    };
    event_loop.run_app(&mut runner)?;
    runner.error.map_or(Ok(()), Err)
}

struct Runner<A> {
    host: Host<A>,
    // Kept between passes so a warm pass allocates nothing.
    canvas: Canvas,
    open: Option<Open>,
    // winit's callbacks can't return one, so it waits here for run.
    error: Option<RunError>,
}

struct Open {
    window: Arc<Window>,
    gpu: Gpu,
    pacer: Pacer,
    /// The shape last set, so it's only set again when a pass changes it.
    cursor: CursorIcon,
    /// Whether the pointer is grabbed, as last set.
    grab: bool,
    /// As last told to the host.
    maximized: bool,
    /// Where the pointer is, as last told to the host.
    pointer: Option<[f32; 2]>,
    /// The last press that could start a double click, and when and where
    /// it went down.
    press: Option<(PointerButton, Instant, [f32; 2])>,
}

impl Open {
    /// Opens at its least size at `zoom`, and never smaller.
    fn new<A: App>(event_loop: &ActiveEventLoop, zoom: f32) -> Result<Self, RunError> {
        let min = min_size::<A>(zoom);
        let attributes = Window::default_attributes()
            .with_title(A::TITLE)
            .with_inner_size(min)
            .with_min_inner_size(min)
            .with_decorations(A::DECORATED)
            // So the app can round the corners of the frame it draws.
            .with_transparent(!A::DECORATED);
        let window = Arc::new(event_loop.create_window(attributes)?);
        let gpu = gpu(&window, scale(&window, zoom), !A::DECORATED)?;
        let pacer = Pacer::new((!compositor_paced(event_loop)).then(|| interval(&window)));
        Ok(Self {
            window,
            gpu,
            pacer,
            cursor: CursorIcon::Default,
            grab: false,
            maximized: false,
            pointer: None,
            press: None,
        })
    }

    /// `event`, as [`Event::PressedAgain`] when it's the second press of a
    /// double click. A third starts over.
    fn double_click<'a>(&mut self, event: Event<'a>, now: Instant) -> Event<'a> {
        match event {
            Event::Pointer(at) => self.pointer = at,
            Event::Pressed(button) => {
                let Some(at) = self.pointer else { return event };
                let again = self.press.take().is_some_and(|(last, then, [x, y])| {
                    last == button
                        && now.saturating_duration_since(then) <= DOUBLE_CLICK_TIME
                        && (at[0] - x).hypot(at[1] - y) <= DOUBLE_CLICK_DISTANCE
                });
                if again {
                    return Event::PressedAgain(button);
                }
                self.press = Some((button, now, at));
            }
            _ => {}
        }
        event
    }

    /// The window's contents in logical pixels, at `scale`.
    fn rect(&self, scale: f64) -> Rect {
        let size = self.window.inner_size().to_logical::<f32>(scale);
        Rect::new(0., 0., size.width, size.height)
    }
}

/// `A`'s least size, in the system's logical pixels at `zoom`.
fn min_size<A: App>(zoom: f32) -> LogicalSize<f32> {
    let [w, h] = A::MIN_SIZE.map(|n| n * zoom);
    LogicalSize::new(w, h)
}

/// Keeps `window` no smaller than `A`'s least size at `zoom`, growing it
/// if it already is, since not every system does so for a new minimum.
fn zoom_min_size<A: App>(window: &Window, zoom: f32) {
    let min = min_size::<A>(zoom);
    window.set_min_inner_size(Some(min));
    let now = window.inner_size().to_logical::<f32>(window.scale_factor());
    if now.width < min.width || now.height < min.height {
        let grown = LogicalSize::new(now.width.max(min.width), now.height.max(min.height));
        // `None` when it was resized at once, and a resize event follows.
        let _ = window.request_inner_size(grown);
    }
}

/// Physical pixels per logical one: the system's for `window`, at `zoom`.
fn scale(window: &Window, zoom: f32) -> f64 {
    window.scale_factor() * f64::from(zoom)
}

/// A new surface on `window`, at its current size and `scale`, `transparent`
/// as [`Gpu::new`] says.
fn gpu(window: &Arc<Window>, scale: f64, transparent: bool) -> Result<Gpu, render::Error> {
    let size = window.inner_size();
    Gpu::new(
        // The surface holds a clone, so the window can't close before the surface is dropped.
        Arc::clone(window),
        [size.width, size.height],
        scale,
        transparent,
    )
}

impl<A> Runner<A> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: RunError) {
        self.error = Some(error);
        event_loop.exit();
    }
}

impl<A: App> ApplicationHandler for Runner<A> {
    // Fires once at startup on desktop.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.open.is_some() {
            return;
        }
        match Open::new::<A>(event_loop, self.host.zoom()) {
            Ok(mut open) => {
                open.maximized = open.window.is_maximized();
                self.host.push(Event::Maximized(open.maximized));
                self.open = Some(open);
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(open) = &mut self.open else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            // A drag sends many of these per refresh. The pacer turns them into one
            // redraw, which resizes the surface to the latest size.
            WindowEvent::Resized(_) => {
                let maximized = open.window.is_maximized();
                if maximized != open.maximized {
                    open.maximized = maximized;
                    self.host.push(Event::Maximized(maximized));
                }
                open.pacer.request();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                open.pacer.set_interval(interval(&open.window));
                open.pacer.request();
            }
            WindowEvent::Moved(_) => open.pacer.set_interval(interval(&open.window)),
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let elapsed = open.pacer.drew(now);
                if self.host.update_zoom() {
                    zoom_min_size::<A>(&open.window, self.host.zoom());
                }
                let scale = scale(&open.window, self.host.zoom());
                let size = open.window.inner_size();
                open.gpu.resize([size.width, size.height], scale);
                let rounded = !A::DECORATED && !open.maximized && open.gpu.transparent();
                let out = self.host.draw(
                    open.rect(scale),
                    narrow(scale),
                    rounded,
                    elapsed,
                    &mut self.canvas,
                );
                if out.again {
                    open.pacer.request();
                }
                if let Some(at) = out.redraw_in.and_then(|after| now.checked_add(after)) {
                    open.pacer.request_at(at);
                }
                if out.cursor != open.cursor {
                    open.cursor = out.cursor;
                    open.window.set_cursor(cursor_icon(out.cursor));
                }
                if out.grab != open.grab {
                    open.grab = out.grab;
                    grab(&open.window, out.grab);
                }
                if let Some(action) = out.action
                    && act(event_loop, &open.window, action)
                {
                    self.host.push(Event::Released(PointerButton::Left));
                    open.pacer.request();
                }
                let atlas = self.host.take_atlas_updates();
                // Without this, Wayland gets a frame per event and shows them all in turn.
                let window = &open.window;
                match open
                    .gpu
                    .draw(&self.canvas, atlas, || window.pre_present_notify())
                {
                    Ok(()) => {}
                    Err(render::Error::Lost) => match gpu(&open.window, scale, !A::DECORATED) {
                        Ok(gpu) => {
                            open.gpu = gpu;
                            self.host.reupload_atlas();
                            open.pacer.request();
                        }
                        Err(error) => self.fail(event_loop, error.into()),
                    },
                    Err(error) => self.fail(event_loop, error.into()),
                }
            }
            // The system lets go of a grab when the window loses focus, so the
            // next pass grabs again if it still wants to.
            WindowEvent::Focused(false) => {
                open.grab = false;
                self.host.push(Event::Unfocused);
                open.pacer.request();
            }
            WindowEvent::Focused(true) => {
                self.host.push(Event::Focused);
                open.pacer.request();
            }
            event => {
                let scale = scale(&open.window, self.host.zoom());
                let now = Instant::now();
                let mut pushed = false;
                translate(&event, scale, |event| {
                    self.host.push(open.double_click(event, now));
                    pushed = true;
                });
                if pushed {
                    open.pacer.request();
                }
            }
        }
    }

    // From a `Wake`.
    fn user_event(&mut self, _: &ActiveEventLoop, (): ()) {
        if let Some(open) = &mut self.open {
            open.pacer.request();
        }
    }

    // Raw motion only matters while grabbed, and otherwise would redraw for
    // every move outside the window.
    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        let Some(open) = &mut self.open else { return };
        if let (true, DeviceEvent::MouseMotion { delta: (x, y) }) = (open.grab, event) {
            self.host.push(Event::Motion([narrow(x), narrow(y)]));
            open.pacer.request();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(open) = &mut self.open else { return };
        let (redraw, due) = open.pacer.poll(Instant::now());
        if redraw {
            open.window.request_redraw();
        }
        event_loop.set_control_flow(due.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    // Every way out passes through here, including a failure.
    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.host.close();
    }
}

/// Passes on what `event` says the user did, if anything, with positions
/// in logical pixels at `scale` physical pixels per logical one.
fn translate(event: &WindowEvent, scale: f64, mut push: impl FnMut(Event)) {
    match event {
        WindowEvent::CursorMoved { position, .. } => {
            let at = position.to_logical::<f32>(scale);
            push(Event::Pointer(Some([at.x, at.y])));
        }
        WindowEvent::CursorLeft { .. } => push(Event::Pointer(None)),
        WindowEvent::MouseInput { state, button, .. } => {
            let button = match button {
                MouseButton::Left => PointerButton::Left,
                MouseButton::Right => PointerButton::Right,
                MouseButton::Middle => PointerButton::Middle,
                MouseButton::Back => PointerButton::Back,
                MouseButton::Forward => PointerButton::Forward,
                MouseButton::Other(_) => return,
            };
            push(match state {
                ElementState::Pressed => Event::Pressed(button),
                ElementState::Released => Event::Released(button),
            });
        }
        WindowEvent::MouseWheel { delta, .. } => push(Event::Scroll(match delta {
            MouseScrollDelta::LineDelta(x, y) => [x * SCROLL_LINE, y * SCROLL_LINE],
            MouseScrollDelta::PixelDelta(by) => {
                let by = by.to_logical::<f32>(scale);
                [by.x, by.y]
            }
        })),
        // A synthetic press is a key already held when the window gained
        // focus, which the user didn't press for this window.
        WindowEvent::KeyboardInput {
            event:
                KeyEvent {
                    logical_key,
                    physical_key,
                    text,
                    state: ElementState::Pressed,
                    repeat,
                    ..
                },
            is_synthetic: false,
            ..
        } => {
            if let Some(key) = key(logical_key) {
                let code = code(*physical_key);
                let repeat = *repeat;
                push(Event::Key { key, code, repeat });
            }
            // Keys like Enter and Backspace type control characters, which
            // are reported as keys instead.
            if let Some(text) = text
                .as_deref()
                .filter(|text| !text.contains(char::is_control))
            {
                push(Event::Text(text));
            }
        }
        WindowEvent::KeyboardInput {
            event:
                KeyEvent {
                    logical_key,
                    physical_key,
                    state: ElementState::Released,
                    ..
                },
            ..
        } => {
            if let Some(key) = key(logical_key) {
                let code = code(*physical_key);
                push(Event::KeyReleased { key, code });
            }
        }
        WindowEvent::ModifiersChanged(modifiers) => {
            let state = modifiers.state();
            let held = [
                (state.shift_key(), Modifiers::SHIFT),
                (state.control_key(), Modifiers::CTRL),
                (state.alt_key(), Modifiers::ALT),
                (state.super_key(), Modifiers::SUPER),
            ];
            let modifiers = held
                .into_iter()
                .filter(|&(down, _)| down)
                .fold(Modifiers::default(), |all, (_, one)| all.with(one));
            push(Event::Modifiers(modifiers));
        }
        _ => {}
    }
}

/// The [`Key`] `key` is, if the editor acts on it.
fn key(key: &keyboard::Key) -> Option<Key> {
    let named = match key {
        keyboard::Key::Named(named) => named,
        keyboard::Key::Character(text) => {
            let mut chars = text.chars();
            let c = chars.next()?;
            // A dead key or input method can type more than one.
            return chars
                .next()
                .is_none()
                .then(|| Key::Char(c.to_lowercase().next().unwrap_or(c)));
        }
        _ => return None,
    };
    Some(match named {
        NamedKey::Shift => Key::Shift,
        NamedKey::Control => Key::Ctrl,
        NamedKey::Alt => Key::Alt,
        NamedKey::Super => Key::Super,
        NamedKey::Enter => Key::Enter,
        NamedKey::Escape => Key::Escape,
        NamedKey::Tab => Key::Tab,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        NamedKey::Insert => Key::Insert,
        NamedKey::ArrowLeft => Key::Left,
        NamedKey::ArrowRight => Key::Right,
        NamedKey::ArrowUp => Key::Up,
        NamedKey::ArrowDown => Key::Down,
        NamedKey::Home => Key::Home,
        NamedKey::End => Key::End,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::PageDown => Key::PageDown,
        NamedKey::Space => Key::Char(' '),
        NamedKey::F1 => Key::F(1),
        NamedKey::F2 => Key::F(2),
        NamedKey::F3 => Key::F(3),
        NamedKey::F4 => Key::F(4),
        NamedKey::F5 => Key::F(5),
        NamedKey::F6 => Key::F(6),
        NamedKey::F7 => Key::F(7),
        NamedKey::F8 => Key::F(8),
        NamedKey::F9 => Key::F(9),
        NamedKey::F10 => Key::F(10),
        NamedKey::F11 => Key::F(11),
        NamedKey::F12 => Key::F(12),
        _ => return None,
    })
}

/// Where `physical` is, if it's a key with a character.
const fn code(physical: PhysicalKey) -> Option<KeyCode> {
    let PhysicalKey::Code(code) = physical else {
        return None;
    };
    let c = match code {
        keyboard::KeyCode::KeyA => 'a',
        keyboard::KeyCode::KeyB => 'b',
        keyboard::KeyCode::KeyC => 'c',
        keyboard::KeyCode::KeyD => 'd',
        keyboard::KeyCode::KeyE => 'e',
        keyboard::KeyCode::KeyF => 'f',
        keyboard::KeyCode::KeyG => 'g',
        keyboard::KeyCode::KeyH => 'h',
        keyboard::KeyCode::KeyI => 'i',
        keyboard::KeyCode::KeyJ => 'j',
        keyboard::KeyCode::KeyK => 'k',
        keyboard::KeyCode::KeyL => 'l',
        keyboard::KeyCode::KeyM => 'm',
        keyboard::KeyCode::KeyN => 'n',
        keyboard::KeyCode::KeyO => 'o',
        keyboard::KeyCode::KeyP => 'p',
        keyboard::KeyCode::KeyQ => 'q',
        keyboard::KeyCode::KeyR => 'r',
        keyboard::KeyCode::KeyS => 's',
        keyboard::KeyCode::KeyT => 't',
        keyboard::KeyCode::KeyU => 'u',
        keyboard::KeyCode::KeyV => 'v',
        keyboard::KeyCode::KeyW => 'w',
        keyboard::KeyCode::KeyX => 'x',
        keyboard::KeyCode::KeyY => 'y',
        keyboard::KeyCode::KeyZ => 'z',
        keyboard::KeyCode::Digit0 => '0',
        keyboard::KeyCode::Digit1 => '1',
        keyboard::KeyCode::Digit2 => '2',
        keyboard::KeyCode::Digit3 => '3',
        keyboard::KeyCode::Digit4 => '4',
        keyboard::KeyCode::Digit5 => '5',
        keyboard::KeyCode::Digit6 => '6',
        keyboard::KeyCode::Digit7 => '7',
        keyboard::KeyCode::Digit8 => '8',
        keyboard::KeyCode::Digit9 => '9',
        keyboard::KeyCode::Backquote => '`',
        keyboard::KeyCode::Minus => '-',
        keyboard::KeyCode::Equal => '=',
        keyboard::KeyCode::BracketLeft => '[',
        keyboard::KeyCode::BracketRight => ']',
        keyboard::KeyCode::Backslash => '\\',
        keyboard::KeyCode::Semicolon => ';',
        keyboard::KeyCode::Quote => '\'',
        keyboard::KeyCode::Comma => ',',
        keyboard::KeyCode::Period => '.',
        keyboard::KeyCode::Slash => '/',
        keyboard::KeyCode::Space => ' ',
        _ => return None,
    };
    Some(KeyCode(c))
}

/// Locks the pointer in place where the system can, otherwise keeps it in
/// the window, and hides it while grabbed.
fn grab(window: &Window, grab: bool) {
    let mode = if grab {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    // A system that refuses both leaves the pointer free, with motion still
    // coming.
    if window.set_cursor_grab(mode).is_err() && grab {
        window.set_cursor_grab(CursorGrabMode::Confined).ok();
    }
    window.set_cursor_visible(!grab);
}

/// Does `action` to `window`. Returns whether the system took the pointer
/// for it, which lets go of the left button without telling the window.
fn act(event_loop: &ActiveEventLoop, window: &Window, action: WindowAction) -> bool {
    match action {
        WindowAction::Move => window.drag_window().is_ok(),
        WindowAction::Resize(edge) => window.drag_resize_window(direction(edge)).is_ok(),
        WindowAction::Minimize => {
            window.set_minimized(true);
            false
        }
        WindowAction::ToggleMaximize => {
            window.set_maximized(!window.is_maximized());
            false
        }
        WindowAction::Close => {
            event_loop.exit();
            false
        }
    }
}

const fn direction(edge: WindowEdge) -> ResizeDirection {
    match edge {
        WindowEdge::North => ResizeDirection::North,
        WindowEdge::NorthEast => ResizeDirection::NorthEast,
        WindowEdge::East => ResizeDirection::East,
        WindowEdge::SouthEast => ResizeDirection::SouthEast,
        WindowEdge::South => ResizeDirection::South,
        WindowEdge::SouthWest => ResizeDirection::SouthWest,
        WindowEdge::West => ResizeDirection::West,
        WindowEdge::NorthWest => ResizeDirection::NorthWest,
    }
}

const fn cursor_icon(cursor: CursorIcon) -> winit::window::CursorIcon {
    match cursor {
        CursorIcon::Default => winit::window::CursorIcon::Default,
        CursorIcon::Pointer => winit::window::CursorIcon::Pointer,
        CursorIcon::Text => winit::window::CursorIcon::Text,
        CursorIcon::ResizeH => winit::window::CursorIcon::EwResize,
        CursorIcon::ResizeV => winit::window::CursorIcon::NsResize,
        CursorIcon::Grab => winit::window::CursorIcon::Grab,
        CursorIcon::Grabbing => winit::window::CursorIcon::Grabbing,
        CursorIcon::NotAllowed => winit::window::CursorIcon::NotAllowed,
        CursorIcon::ResizeNwSe => winit::window::CursorIcon::NwseResize,
        CursorIcon::ResizeNeSw => winit::window::CursorIcon::NeswResize,
    }
}

/// The time between refreshes of the monitor `window` is on.
fn interval(window: &Window) -> Duration {
    window
        .current_monitor()
        .and_then(|monitor| monitor.refresh_rate_millihertz())
        .filter(|&millihertz| millihertz > 0)
        .map_or(FALLBACK_INTERVAL, |millihertz| {
            Duration::from_nanos(1_000_000_000_000 / u64::from(millihertz))
        })
}

/// Whether the compositor holds each redraw until it has shown the last, which
/// winit does on Wayland once [`Window::pre_present_notify`] is called.
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
))]
fn compositor_paced(event_loop: &ActiveEventLoop) -> bool {
    use winit::platform::wayland::ActiveEventLoopExtWayland;
    event_loop.is_wayland()
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
)))]
const fn compositor_paced(_: &ActiveEventLoop) -> bool {
    false
}
