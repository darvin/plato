#![recursion_limit = "1024"]

#[macro_use]
extern crate error_chain;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate downcast_rs;
extern crate unicode_normalization;
extern crate libc;
extern crate regex;
extern crate chrono;
extern crate sdl2;
extern crate fnv;
extern crate png;
extern crate isbn;
extern crate titlecase;

#[macro_use]
mod geom;
mod unit;
mod color;
mod framebuffer;
mod input;
mod gesture;
mod view;
mod battery;
mod device;
mod font;
mod helpers;
mod document;
mod metadata;
mod settings;
mod frontlight;
mod symbolic_path;
mod app;

mod errors {
    error_chain!{
        foreign_links {
            Io(::std::io::Error);
            ParseInt(::std::num::ParseIntError);
        }
        links {
            Font(::font::Error, ::font::ErrorKind);
        }
    }
}

use std::thread;
use std::path::Path;
use std::fs::File;
use std::sync::mpsc;
use std::collections::VecDeque;
use std::time::Duration;
use fnv::FnvHashMap;
use chrono::Local;
use png::HasParameters;
use sdl2::event::Event as SdlEvent;
use sdl2::mouse::MouseButton;
use sdl2::keyboard::Keycode;
use sdl2::render::{WindowCanvas, BlendMode};
use sdl2::pixels::{Color as SdlColor, PixelFormatEnum};
use sdl2::rect::Point as SdlPoint;
use framebuffer::{Framebuffer, UpdateMode};
use input::{DeviceEvent, FingerStatus};
use view::{View, Event, ViewId, EntryId, render, render_no_wait, handle_event, fill_crack};
use view::home::Home;
use view::reader::Reader;
use view::notification::Notification;
use view::frontlight::FrontlightWindow;
use view::key::KeyKind;
use view::common::{locate, locate_by_id, overlapping_rectangle};
use geom::{Rectangle, LinearDir};
use gesture::gesture_events;
use device::CURRENT_DEVICE;
use helpers::{load_json, save_json};
use metadata::{Metadata, METADATA_FILENAME};
use settings::{Settings, SETTINGS_PATH};
use frontlight::{Frontlight, FakeFrontlight};
use battery::{Battery, FakeBattery};
use font::Fonts;
use app::Context;
use errors::*;

pub const APP_NAME: &str = "Plato";

const CLOCK_REFRESH_INTERVAL_MS: u64 = 60*1000;

pub fn build_context() -> Result<Context> {
    let settings = load_json::<Settings, _>(SETTINGS_PATH)?;
    let path = settings.library_path.join(METADATA_FILENAME);
    let metadata = load_json::<Metadata, _>(path)?;
    let frontlight = Box::new(FakeFrontlight::new()) as Box<Frontlight>;
    let battery = Box::new(FakeBattery::new()) as Box<Battery>;
    let fonts = Fonts::load()?;
    Ok(Context::new(settings, metadata, fonts, frontlight, battery))
}

#[inline]
fn seconds(timestamp: u32) -> f64 {
    timestamp as f64 / 1000.0
}

#[inline]
pub fn device_event(event: SdlEvent) -> Option<DeviceEvent> {
    match event {
        SdlEvent::MouseButtonDown { timestamp, x, y, .. } => 
            Some(DeviceEvent::Finger { id: 0,
                                       status: FingerStatus::Down,
                                       position: pt!(x, y),
                                       time: seconds(timestamp) }),
        SdlEvent::MouseButtonUp { timestamp, x, y, .. } =>
            Some(DeviceEvent::Finger { id: 0,
                                       status: FingerStatus::Up,
                                       position: pt!(x, y),
                                       time: seconds(timestamp) }),
        SdlEvent::MouseMotion { timestamp, x, y, .. } =>
            Some(DeviceEvent::Finger { id: 0,
                                       status: FingerStatus::Motion,
                                       position: pt!(x, y),
                                       time: seconds(timestamp) }),
        _ => None
    }
}

impl Framebuffer for WindowCanvas {
    fn set_pixel(&mut self, x: u32, y: u32, color: u8) {
        self.set_draw_color(SdlColor::RGB(color, color, color));
        self.draw_point(SdlPoint::new(x as i32, y as i32)).unwrap();
    }

    fn set_blended_pixel(&mut self, x: u32, y: u32, color: u8, alpha: f32) {
        self.set_draw_color(SdlColor::RGBA(color, color, color, (alpha * 255.0) as u8));
        self.draw_point(SdlPoint::new(x as i32, y as i32)).unwrap();
    }

    fn update(&mut self, _rect: &Rectangle, _mode: UpdateMode) -> Result<u32> {
        self.present();
        Ok(1)
    }

    fn wait(&self, _: u32) -> Result<i32> {
        Ok(1)
    }

    fn save(&self, path: &str) -> Result<()> {
        let (width, height) = self.dims();
        let file = File::create(path).chain_err(|| "Can't create output file.")?;
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set(png::ColorType::RGB).set(png::BitDepth::Eight);
        let mut writer = encoder.write_header().chain_err(|| "Can't write header.")?;
        let data = self.read_pixels(self.viewport(), PixelFormatEnum::RGB24).unwrap_or_default();
        writer.write_image_data(&data).chain_err(|| "Can't write data to file.")?;
        Ok(())
    }

    fn toggle_inverted(&mut self) {}

    fn toggle_monochrome(&mut self) {}

    fn dims(&self) -> (u32, u32) {
        self.window().size()
    }
}

pub fn run() -> Result<()> {
    let mut context = build_context()?;
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let (width, height) = CURRENT_DEVICE.dims;
    let window = video_subsystem
                 .window("Plato Emulator", width, height)
                 .position_centered()
                 .build()
                 .unwrap();

    let mut fb = window.into_canvas().software().build().unwrap();
    fb.set_blend_mode(BlendMode::Blend);

    let (tx, rx) = mpsc::channel();
    let (ty, ry) = mpsc::channel();
    let touch_screen = gesture_events(ry);

    let tx2 = tx.clone();
    thread::spawn(move || {
        while let Ok(evt) = touch_screen.recv() {
            tx2.send(evt).unwrap();
        }
    });

    let tx3 = tx.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(CLOCK_REFRESH_INTERVAL_MS));
            tx3.send(Event::ClockTick).unwrap();
        }
    });

    let fb_rect = fb.rect();

    let mut history: Vec<Box<View>> = Vec::new();
    let mut view: Box<View> = Box::new(Home::new(fb_rect, &tx, &mut context)?);

    let mut updating = FnvHashMap::default();

    println!("{} is running on a Kobo {}.", APP_NAME,
                                            CURRENT_DEVICE.model);
    println!("The framebuffer resolution is {} by {}.", fb_rect.width(),
                                                     fb_rect.height());

    let mut bus = VecDeque::with_capacity(4);

    'outer: loop {
        if let Some(sdl_evt) = sdl_context.event_pump().unwrap().wait_event_timeout(20) {
            match sdl_evt {
                SdlEvent::Quit { .. } => break,
                SdlEvent::KeyDown { keycode: Some(keycode), .. } => {
                    match keycode {
                        Keycode::LShift | Keycode::RShift => {
                            tx.send(Event::Key(KeyKind::Shift)).unwrap();
                        },
                        Keycode::LAlt => {
                            tx.send(Event::Key(KeyKind::Combine)).unwrap();
                        },
                        Keycode::RAlt => {
                            tx.send(Event::Key(KeyKind::Alternate)).unwrap();
                        },
                        Keycode::Return => {
                            tx.send(Event::Key(KeyKind::Return)).unwrap();
                        },
                        Keycode::Left => {
                            tx.send(Event::Key(KeyKind::Move(LinearDir::Backward))).unwrap();
                        },
                        Keycode::Right => {
                            tx.send(Event::Key(KeyKind::Move(LinearDir::Forward))).unwrap();
                        },
                        Keycode::Backspace => {
                            tx.send(Event::Key(KeyKind::Delete(LinearDir::Backward))).unwrap();
                        },
                        Keycode::Delete => {
                            tx.send(Event::Key(KeyKind::Delete(LinearDir::Forward))).unwrap();
                        },
                        Keycode::Escape => break,
                        _ => {
                            let name = keycode.name();
                            if name.len() == 1 {
                                let c = name.chars().next().unwrap()
                                            .to_lowercase().next().unwrap();
                                tx.send(Event::Key(KeyKind::Output(c))).unwrap();
                            }
                        },

                    }
                },
                _ => {
                    if let Some(dev_evt) = device_event(sdl_evt) {
                        ty.send(dev_evt).unwrap();
                    }
                },
            }
        }

        while let Ok(evt) = rx.recv_timeout(Duration::from_millis(20)) {
            match evt {
                Event::Render(mut rect, mode) => {
                    render(view.as_ref(), &mut rect, &mut fb, &mut context.fonts, &mut updating);
                    if let Ok(tok) = fb.update(&rect, mode) {
                        updating.insert(tok, rect);
                    }
                },
                Event::RenderNoWait(mut rect, mode) => {
                    render_no_wait(view.as_ref(), &mut rect, &mut fb, &mut context.fonts, &mut updating);
                    if let Ok(tok) = fb.update(&rect, mode) {
                        updating.insert(tok, rect);
                    }
                },
                Event::Expose(mut rect) => {
                    fill_crack(view.as_ref(), &mut rect, &mut fb, &mut context.fonts, &mut updating);
                    if let Ok(tok) = fb.update(&rect, UpdateMode::Gui) {
                        updating.insert(tok, rect);
                    }
                },
                Event::Open(info) => {
                    let info2 = info.clone();
                    if let Some(r) = Reader::new(fb_rect, *info, &tx, &mut context) {
                        history.push(view as Box<View>);
                        view = Box::new(r) as Box<View>;
                    } else {
                        handle_event(view.as_mut(), &Event::Invalid(info2), &tx, &mut bus, &mut context);
                    }
                },
                Event::Back => {
                    if let Some(v) = history.pop() {
                        view = v;
                    }
                    view.handle_event(&evt, &tx, &mut bus, &mut context);
                },
                Event::Show(ViewId::Frontlight) => {
                    if !context.settings.frontlight {
                        continue;
                    }
                    let flw = FrontlightWindow::new(&mut context);
                    tx.send(Event::Render(*flw.rect(), UpdateMode::Gui)).unwrap();
                    view.children_mut().push(Box::new(flw) as Box<View>);
                },
                Event::Close(ViewId::Frontlight) => {
                    if let Some(index) = locate::<FrontlightWindow>(view.as_ref()) {
                        let rect = *view.child(index).rect();
                        view.children_mut().remove(index);
                        tx.send(Event::Expose(rect)).unwrap();
                    }
                },
                Event::Close(id) => {
                    if let Some(index) = locate_by_id(view.as_ref(), id) {
                        let rect = overlapping_rectangle(view.as_ref());
                        tx.send(Event::Expose(rect)).unwrap();
                        view.children_mut().remove(index);
                    }
                },
                Event::Select(EntryId::ToggleInverted) => {
                    fb.toggle_inverted();
                    context.inverted = !context.inverted;
                    tx.send(Event::Render(fb_rect, UpdateMode::Gui)).unwrap();
                },
                Event::Select(EntryId::ToggleMonochrome) => {
                    fb.toggle_monochrome();
                    context.monochrome = !context.monochrome;
                    tx.send(Event::Render(fb_rect, UpdateMode::Gui)).unwrap();
                },
                Event::Select(EntryId::TakeScreenshot) => {
                    let name = Local::now().format("screenshot-%Y%m%d_%H%M%S.png");
                    let msg = match fb.save(&name.to_string()) {
                        Err(e) => format!("Couldn't take screenshot: {}).", e),
                        Ok(_) => format!("Saved {}.", name),
                    };
                    let notif = Notification::new(ViewId::TakeScreenshotNotif,
                                                  msg,
                                                  &mut context.notification_index,
                                                  &mut context.fonts,
                                                  &tx);
                    view.children_mut().push(Box::new(notif) as Box<View>);
                },
                Event::Select(EntryId::Quit) => {
                    break 'outer;
                },
                _ => {
                    handle_event(view.as_mut(), &evt, &tx, &mut bus, &mut context);
                },
            }

            while let Some(ce) = bus.pop_front() {
                tx.send(ce).unwrap();
            }
        }
    }

    let path = context.settings.library_path.join(METADATA_FILENAME);
    save_json(&context.metadata, path).chain_err(|| "Can't save metadata.")?;

    let path = Path::new(SETTINGS_PATH);
    save_json(&context.settings, path).chain_err(|| "Can't save settings.")?;

    Ok(())
}

quick_main!(run);
