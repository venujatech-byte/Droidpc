mod frame;
mod texture;

use eframe::egui::{self, Color32, Key, PointerButton, Pos2, Rect, Vec2};
use eframe::glow::HasContext;
use frame::Frame;
use std::{
    env,
    fs::{File, OpenOptions},
    io::{self, Write},
    os::unix::{
        fs::{FileTypeExt, OpenOptionsExt},
        net::UnixStream,
    },
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

type Latest = Arc<Mutex<Result<Option<Arc<Frame>>, String>>>;

struct Window {
    latest: Latest,
    texture: Option<texture::Texture>,
    size: [usize; 2],
    input: mpsc::SyncSender<String>,
    pressed: Option<([u32; 2], Instant)>,
    started: Instant,
    activity: Arc<AtomicBool>,
    capture_thread: thread::Thread,
    fps_started: Instant,
    fps_count: u64,
    fps: f32,
    error: Option<String>,
}

fn position(rect: Rect, size: [usize; 2], p: Pos2) -> Option<[u32; 2]> {
    if !rect.contains(p) || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }
    Some([
        (((p.x - rect.left()) / rect.width() * size[0] as f32) as u32)
            .min(size[0].saturating_sub(1) as u32),
        (((p.y - rect.top()) / rect.height() * size[1] as f32) as u32)
            .min(size[1].saturating_sub(1) as u32),
    ])
}

impl Window {
    fn send(&self, command: String) {
        self.activity.store(true, Ordering::Relaxed);
        self.capture_thread.unpark();
        if let Err(error) = self.input.try_send(command) {
            eprintln!("Android input queue: {error}");
        }
    }

    fn input(&mut self, ctx: &egui::Context, rect: Rect) {
        let events = ctx.input(|input| input.events.clone());
        for event in events {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    ..
                } => {
                    if pressed {
                        self.pressed = position(rect, self.size, pos)
                            .map(|p| (p, Instant::now()));
                    } else if let Some((start, when)) = self.pressed.take()
                        && let Some(end) = position(rect, self.size, pos)
                    {
                        if start[0].abs_diff(end[0]) + start[1].abs_diff(end[1])
                            < 12
                        {
                            self.send(format!("tap {} {}", end[0], end[1]));
                        } else {
                            self.send(format!(
                                "swipe {} {} {} {} {}",
                                start[0],
                                start[1],
                                end[0],
                                end[1],
                                when.elapsed().as_millis().clamp(80, 1000)
                            ));
                        }
                    }
                }
                egui::Event::PointerButton {
                    button: PointerButton::Secondary,
                    pressed: true,
                    ..
                } => self.send("key 4".into()),
                egui::Event::WindowFocused(false) => self.pressed = None,
                egui::Event::Key {
                    key, pressed: true, ..
                } => {
                    let code = match key {
                        Key::Escape => Some(4),
                        Key::Home => Some(3),
                        Key::Backspace => Some(67),
                        Key::Delete => Some(112),
                        Key::Enter => Some(66),
                        Key::Tab => Some(61),
                        Key::ArrowLeft => Some(21),
                        Key::ArrowUp => Some(19),
                        Key::ArrowRight => Some(22),
                        Key::ArrowDown => Some(20),
                        _ => None,
                    };
                    if let Some(code) = code {
                        self.send(format!("key {code}"));
                    }
                }
                egui::Event::Text(text) | egui::Event::Paste(text) => {
                    if text.len() <= 4000
                        && text
                            .bytes()
                            .all(|b| b.is_ascii() && !b.is_ascii_control())
                    {
                        self.send(format!("text {}", text.replace(' ', "%s")));
                    }
                }
                egui::Event::MouseWheel { delta, .. } if delta.y != 0.0 => {
                    if let Some(point) = ctx
                        .input(|i| i.pointer.hover_pos())
                        .and_then(|p| position(rect, self.size, p))
                    {
                        let end = (point[1] as f32
                            + delta.y.signum() * self.size[1] as f32 * 0.25)
                            .clamp(0.0, self.size[1].saturating_sub(1) as f32)
                            as u32;
                        self.send(format!(
                            "swipe {} {} {} {} 180",
                            point[0], point[1], point[0], end
                        ));
                    }
                }
                _ => {}
            }
        }
    }
}

impl eframe::App for Window {
    fn update(
        &mut self,
        ctx: &egui::Context,
        render_frame: &mut eframe::Frame,
    ) {
        let next = {
            let mut latest = self.latest.lock().unwrap();
            match &mut *latest {
                Ok(frame) => frame.take(),
                Err(error) => {
                    self.error = Some(error.clone());
                    None
                }
            }
        };
        if let Some(frame) = next {
            if self.size != frame.size {
                self.pressed = None;
            }
            self.size = frame.size;
            if self.error.is_none() {
                match texture::upload(self.texture.take(), &frame, render_frame)
                {
                    Ok(texture) => {
                        self.texture = Some(texture);
                    }
                    Err(error) => {
                        eprintln!("{error}");
                        self.error = Some(error);
                    }
                }
            }
            self.fps_count += 1;
            let elapsed = self.fps_started.elapsed().as_secs_f32();
            if elapsed >= 1.0 {
                self.fps = self.fps_count as f32 / elapsed;
                self.fps_count = 0;
                self.fps_started = Instant::now();
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "Droidpc Android · {:.0} fps",
                    self.fps
                )));
            }
        }
        egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::BLACK)).show(ctx, |ui| {
            let available = ui.max_rect();
            if let Some(texture) = &self.texture {
                let frame = Vec2::new(self.size[0] as f32, self.size[1] as f32);
                let scale = (available.width() / frame.x).min(available.height() / frame.y);
                let rect = Rect::from_center_size(available.center(), frame * scale);
                ui.painter().image(texture.id, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                self.input(ctx, rect);
            } else {
                let message = self.error.as_deref().unwrap_or(if self.started.elapsed() > Duration::from_secs(90) {
                    "No Android frames received. Check the capture log printed in the terminal."
                } else { "Starting Android… waiting for the first frame" });
                ui.centered_and_justified(|ui| { ui.label(message); });
                ctx.request_repaint_after(Duration::from_secs(1));
            }
            if let Some(error) = &self.error {
                ui.painter().text(available.left_top(), egui::Align2::LEFT_TOP, error,
                    egui::FontId::proportional(16.0), Color32::LIGHT_RED);
            }
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: droidpc-window SCREEN_FIFO INPUT_FIFO".into());
    }
    let screen = PathBuf::from(&args[0]);
    let input = PathBuf::from(&args[1]);
    for path in [&screen, &input] {
        if !std::fs::symlink_metadata(path)?.file_type().is_fifo() {
            return Err(format!("{} is not a FIFO", path.display()).into());
        }
    }
    // Keep the reader alive between guest captures; a FIFO needs no video codec.
    let mut stream = OpenOptions::new().read(true).write(true).open(screen)?;
    let latest: Latest = Arc::new(Mutex::new(Ok(None)));
    let (sender, receiver) = mpsc::sync_channel::<String>(64);
    thread::spawn(move || {
        let mut writer: Option<File> = None;
        for command in receiver {
            if writer.is_none() {
                writer = OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&input)
                    .ok();
            }
            if let Some(file) = &mut writer {
                // Commands fit PIPE_BUF, so nonblocking writes stay atomic.
                let line = format!("{command}\n");
                if file.write_all(line.as_bytes()).is_err() {
                    writer = None;
                }
            }
        }
    });
    let fps: u32 = env::var("DROIDPC_WINDOW_FPS")
        .unwrap_or_else(|_| "60".into())
        .parse()?;
    if !(1..=240).contains(&fps) {
        return Err("DROIDPC_WINDOW_FPS must be between 1 and 240".into());
    }
    let display_stream = match env::var("DROIDPC_CAPTURE_SOURCE")
        .as_deref()
        .unwrap_or("snapshot")
    {
        "display" => true,
        "snapshot" => false,
        _ => {
            return Err(
                "DROIDPC_CAPTURE_SOURCE must be display or snapshot".into()
            );
        }
    };
    let activity = Arc::new(AtomicBool::new(false));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 540.0])
            .with_min_inner_size([320.0, 200.0]),
        renderer: eframe::Renderer::Glow,
        hardware_acceleration: eframe::HardwareAcceleration::Required,
        vsync: true,
        ..Default::default()
    };
    // Fail before booting if the desktop socket cannot be reached.
    if let (Some(runtime), Some(display)) = (
        env::var_os("XDG_RUNTIME_DIR"),
        env::var_os("WAYLAND_DISPLAY"),
    ) {
        UnixStream::connect(PathBuf::from(runtime).join(display))?;
    }
    eframe::run_native(
        "Droidpc Android",
        options,
        Box::new(move |cc| {
            if let Some(gl) = &cc.gl {
                // The window's OpenGL context is current during app creation.
                let renderer =
                    unsafe { gl.get_parameter_string(eframe::glow::RENDERER) };
                eprintln!(
                    "OpenGL renderer: {renderer}; capture ceiling: {fps} fps; continuous source: {display_stream}"
                );
            }
            let state = latest.clone();
            let ctx = cc.egui_ctx.clone();
            let capture_activity = activity.clone();
            let capture = thread::spawn(move || {
                let mut previous: Option<Arc<Frame>> = None;
                let mut changed_at = Instant::now();
                loop {
                    let began = Instant::now();
                    let next = frame::read_any_frame(&mut stream);
                    match next {
                        Ok(frame) => {
                            if previous.is_none() {
                                eprintln!(
                                    "Android first frame: {}x{}",
                                    frame.size[0], frame.size[1]
                                );
                            }
                            let changed = previous.as_ref().is_none_or(|old| {
                                old.size != frame.size
                                    || old.layout != frame.layout
                                    || old.pixels != frame.pixels
                            });
                            if changed {
                                changed_at = Instant::now();
                                let frame = Arc::new(frame);
                                *state.lock().unwrap() =
                                    Ok(Some(frame.clone()));
                                previous = Some(frame);
                                ctx.request_repaint();
                            }
                        }
                        Err(error) => {
                            let message = format!(
                                "Android capture stream failed: {error}"
                            );
                            eprintln!("{message}");
                            *state.lock().unwrap() = Err(message);
                            ctx.request_repaint();
                            break;
                        }
                    }
                    if capture_activity.swap(false, Ordering::Relaxed) {
                        changed_at = Instant::now();
                    }
                    let interval = if previous.as_ref().is_some_and(|frame| frame.layout == frame::PixelLayout::Rgba)
                        && changed_at.elapsed() > Duration::from_secs(1)
                    {
                        Duration::from_millis(250)
                    } else {
                        Duration::from_secs_f64(1.0 / f64::from(fps))
                    };
                    thread::park_timeout(
                        interval.saturating_sub(began.elapsed()),
                    );
                }
            });
            let capture_thread = capture.thread().clone();
            Ok(Box::new(Window {
                latest,
                texture: None,
                size: [0, 0],
                input: sender,
                pressed: None,
                started: Instant::now(),
                activity,
                capture_thread,
                fps_started: Instant::now(),
                fps_count: 0,
                fps: 0.0,
                error: None,
            }))
        }),
    )
    .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_mapping_rejects_letterbox_and_clamps_edges() {
        let rect =
            Rect::from_min_size(Pos2::new(25.0, 0.0), Vec2::new(50.0, 100.0));
        assert_eq!(
            position(rect, [1000, 2000], Pos2::new(50.0, 50.0)),
            Some([500, 1000])
        );
        assert_eq!(position(rect, [1000, 2000], Pos2::new(0.0, 50.0)), None);
        assert_eq!(position(rect, [1000, 2000], rect.max), Some([999, 1999]));
    }

    #[test]
    fn native_events_preserve_taps_keys_and_ascii_text() {
        let (sender, receiver) = mpsc::sync_channel(8);
        let mut window = Window {
            latest: Arc::new(Mutex::new(Ok(None))),
            texture: None,
            size: [1000, 2000],
            input: sender,
            pressed: None,
            started: Instant::now(),
            activity: Arc::new(AtomicBool::new(false)),
            capture_thread: thread::current(),
            fps_started: Instant::now(),
            fps_count: 0,
            fps: 0.0,
            error: None,
        };
        let ctx = egui::Context::default();
        let point = Pos2::new(50.0, 50.0);
        ctx.begin_pass(egui::RawInput {
            events: vec![
                egui::Event::PointerButton {
                    pos: point,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: point,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::Text("Hello world!".into()),
                egui::Event::Text("λ".into()),
                egui::Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        });
        window.input(
            &ctx,
            Rect::from_min_size(Pos2::new(25.0, 0.0), Vec2::new(50.0, 100.0)),
        );
        let _ = ctx.end_pass();
        assert_eq!(
            receiver.try_iter().collect::<Vec<_>>(),
            ["tap 500 1000", "text Hello%sworld!", "key 4"]
        );
    }
}
