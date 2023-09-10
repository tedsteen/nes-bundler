#![deny(clippy::all)]

use std::cell::RefCell;
use std::rc::Rc;

use crate::{
    gameloop::TimeTrait,
    input::{JoypadInput, KeyEvent},
    settings::gui::{Gui, ToGuiEvent},
};
use anyhow::{Context, Result};
use audio::Audio;
use egui::{epaint::ImageDelta, Color32, ColorImage, ImageData, TextureOptions};
use gameloop::{GameLoop, Time};
use settings::gui::{EmptyGuiComponent, GuiComponent};

use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
use base64::Engine;

use input::{
    keys::{KeyCode, Mod},
    Input, Inputs,
};
use palette::NTSC_PAL;
use rusticnes_core::cartridge::mapper_from_file;
use rusticnes_core::nes::NesState;
use sdl2::{clipboard::ClipboardUtil, video::FullscreenType, Sdl};
use serde::Deserialize;
use settings::{Settings, MAX_PLAYERS};

mod audio;
#[cfg(feature = "debug")]
mod debug;
mod egui_sdl2;
mod gameloop;
mod input;
#[cfg(feature = "netplay")]
mod netplay;
mod palette;
mod settings;

type Fps = u32;
const FPS: Fps = 60;
const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const ZOOM: f32 = 3.0;

pub fn start_nes(cart_data: Vec<u8>, sample_rate: u64) -> Result<NesState> {
    let rom_data = match std::env::var("ROM_FILE") {
        Ok(rom_file) => {
            std::fs::read(&rom_file).context(format!("Could not read ROM {}", rom_file))?
        }
        Err(_e) => cart_data.to_vec(),
    };

    let mapper = mapper_from_file(rom_data.as_slice())
        .map_err(anyhow::Error::msg)
        .context("Failed to load ROM")?;
    #[cfg(feature = "debug")]
    mapper.print_debug_status();
    let mut nes = NesState::new(mapper);
    nes.power_on();
    nes.apu.set_sample_rate(sample_rate);

    Ok(nes)
}

pub struct Bundle {
    config: BuildConfiguration,
    rom: Vec<u8>,
}

#[cfg(feature = "zip-bundle")]
fn load_bundle_from_zip(zip_file: std::io::Result<std::fs::File>) -> Result<Bundle> {
    if let Ok(zip_file) = zip_file {
        let mut zip = zip::ZipArchive::new(zip_file)?;
        let config: BuildConfiguration = serde_yaml::from_reader(
            zip.by_name("config.yaml")
                .context("config.yaml not found in bundle.zip")?,
        )?;

        let mut rom = Vec::new();
        std::io::copy(
            &mut zip
                .by_name("rom.nes")
                .context("rom.nes not found in bundle.zip")?,
            &mut rom,
        )?;
        Ok(Bundle { config, rom })
    } else {
        let folder = rfd::FileDialog::new()
            .set_title("Files to bundle")
            .set_directory(".")
            .pick_folder()
            .context("No bundle to load")?;

        let mut config_path = folder.clone();
        config_path.push("config.yaml");
        let mut config_file = std::fs::File::open(config_path)
            .context(format!("config.yaml not found in {:?}", folder))?;

        let mut rom_path = folder.clone();
        rom_path.push("rom.nes");
        let mut rom_file =
            std::fs::File::open(rom_path).context(format!("rom.nes not found in {:?}", folder))?;

        let mut zip = zip::ZipWriter::new(
            std::fs::File::create("bundle.zip").context("Could not create bundle.zip")?,
        );
        zip.start_file("config.yaml", Default::default())?;
        std::io::copy(&mut config_file, &mut zip)?;

        zip.start_file("rom.nes", Default::default())?;
        std::io::copy(&mut rom_file, &mut zip)?;

        zip.finish()?;

        // Try again with newly created bundle.zip
        load_bundle_from_zip(std::fs::File::open("bundle.zip"))
    }
}
fn load_bundle() -> Result<Bundle> {
    #[cfg(not(feature = "zip-bundle"))]
    return Ok(Bundle {
        config: serde_yaml::from_str(include_str!("../config/config.yaml"))?,
        rom: include_bytes!("../config/rom.nes").to_vec(),
    });
    #[cfg(feature = "zip-bundle")]
    {
        let res = load_bundle_from_zip(std::fs::File::open("bundle.zip"));
        if let Err(e) = &res {
            log::error!("Could not load the bundle: {:?}", e);
        }
        res
    }
}
#[derive(Deserialize, Debug)]
pub struct BuildConfiguration {
    window_title: String,
    default_settings: Settings,
    #[cfg(feature = "netplay")]
    netplay: netplay::NetplayBuildConfiguration,
}

fn check_and_set_fullscreen(
    window: &mut sdl2::video::Window,
    key_mod: Mod,
    key_code: KeyCode,
) -> bool {
    let mut flip = |fs_type: FullscreenType| {
        let fs_type = if window.fullscreen_state() == FullscreenType::Off {
            fs_type
        } else {
            FullscreenType::Off
        };
        window.set_fullscreen(fs_type).unwrap();
    };

    #[cfg(target_os = "macos")]
    if key_mod.contains(Mod::LGUIMOD) && (key_code == KeyCode::F || key_code == KeyCode::Return) {
        flip(FullscreenType::True);
        return true;
    }

    #[cfg(not(target_os = "macos"))]
    if (key_mod.contains(Mod::LALTMOD | Mod::RALTMOD) && key_code == KeyCode::Return)
        || key_code == KeyCode::F11
    {
        flip(FullscreenType::True);
        return true;
    };
    false
}

fn main() -> Result<()> {
    env_logger::init();

    // This is required for certain controllers to work on Windows without the
    // video subsystem enabled:
    sdl2::hint::set("SDL_JOYSTICK_THREAD", "1");
    let sdl_context: Sdl = sdl2::init().map_err(anyhow::Error::msg)?;
    let video = sdl_context.video().unwrap();
    let gl_attr = video.gl_attr();
    gl_attr.set_context_profile(sdl2::video::GLProfile::Core);
    gl_attr.set_context_version(3, 0);

    let bundle = load_bundle()?;
    #[cfg(feature = "netplay")]
    if std::env::args()
        .collect::<String>()
        .contains(&"--print-netplay-id".to_string())
    {
        if let Some(id) = bundle.config.netplay.netplay_id {
            println!("{id}");
        }
        std::process::exit(0);
    }

    let window = video
        .window(
            &bundle.config.window_title,
            (WIDTH as f32 * ZOOM) as u32,
            (HEIGHT as f32 * ZOOM) as u32,
        )
        .opengl()
        .resizable()
        .allow_highdpi()
        .build()
        .unwrap();
    let gl_context = window.gl_create_context().unwrap();
    window
        .subsystem()
        .gl_set_swap_interval(sdl2::video::SwapInterval::LateSwapTearing)
        .or_else(|_| {
            window
                .subsystem()
                .gl_set_swap_interval(sdl2::video::SwapInterval::VSync)
        })
        .expect("Could not gl_set_swap_interval(...)");

    let (gl, window, mut events_loop, _gl_context) = {
        let gl = unsafe {
            glow::Context::from_loader_function(|s| video.gl_get_proc_address(s) as *const _)
        };
        let event_loop = sdl_context.event_pump().unwrap();
        (gl, window, event_loop, gl_context)
    };
    let gl = std::sync::Arc::new(gl);
    let egui_glow = egui_sdl2::EguiGlow::new(&window, gl.clone(), None);

    let settings = Rc::new(RefCell::new(Settings::new(
        bundle.config.default_settings.clone(),
    )));

    let audio = Audio::new(&sdl_context, settings.clone())?;
    let nes = start_nes(bundle.rom.clone(), audio.stream.get_sample_rate() as u64)?;
    let state = LocalGameState::new(nes)?;

    let state_handler = LocalStateHandler {
        state,
        gui: EmptyGuiComponent::new(),
    };

    #[cfg(feature = "netplay")]
    let state_handler = netplay::NetplayStateHandler::new(
        state_handler,
        &bundle,
        &mut settings.borrow_mut().netplay_id,
    );

    let inputs = Inputs::new(&sdl_context, bundle.config.default_settings.input.selected);
    let input = Input::new(inputs, settings.clone());

    let game_runner = GameRunner::new(Box::new(state_handler))?;

    let nes_texture_options = TextureOptions {
        magnification: egui::TextureFilter::Nearest,
        minification: egui::TextureFilter::Nearest,
    };

    let no_image = ImageData::Color(ColorImage::new([0, 0], Color32::TRANSPARENT));

    let nes_texture = egui_glow.egui_ctx.load_texture(
        "nes",
        ImageData::Color(ColorImage::new(
            [WIDTH as usize, HEIGHT as usize],
            Color32::BLACK,
        )),
        nes_texture_options,
    );

    struct GameLoopState {
        game_runner: GameRunner,
        window: sdl2::video::Window,
        egui_glow: egui_sdl2::EguiGlow,
        gui: Gui,
        settings: Rc<RefCell<Settings>>,
        #[cfg(feature = "debug")]
        debug: debug::Debug,
        audio: Audio,
        input: Input,
        clipboard: ClipboardUtil,
    }

    let mut game_loop: GameLoop<GameLoopState, Time> = GameLoop::new(
        GameLoopState {
            game_runner,
            window,
            egui_glow,
            gui: Gui::new(true),
            settings,
            #[cfg(feature = "debug")]
            debug: debug::Debug {
                settings: debug::DebugSettings::new(),
                gui: debug::gui::DebugGui::new(),
            },
            audio,
            input,
            clipboard: sdl_context.video().unwrap().clipboard(),
        },
        FPS,
        0.08,
    );
    let run_frame = Rc::new(std::sync::Mutex::new(|events: Vec<sdl2::event::Event>| {
        game_loop.next_frame(
            |g, _| {
                let loop_state = &mut g.game;
                let egui_glow = &mut loop_state.egui_glow;
                let game_runner = &mut loop_state.game_runner;

                #[allow(unused_mut)] //debug feature needs this
                let mut fps = game_runner.advance(&loop_state.input);
                #[cfg(feature = "debug")]
                if loop_state.debug.settings.override_fps {
                    fps = loop_state.debug.settings.fps;
                }

                // No need to update graphics or audio more than once per update
                let new_frame = game_runner.get_frame().unwrap_or_else(|| no_image.clone());
                egui_glow.egui_ctx.tex_manager().write().set(
                    nes_texture.id(),
                    ImageDelta::full(new_frame, nes_texture_options),
                );

                loop_state
                    .audio
                    .stream
                    .push_samples(game_runner.state_handler.consume_samples().as_slice());

                g.set_updates_per_second(fps);
            },
            |g, extra| {
                if log::max_level() == log::Level::Trace && Time::now().sub(&g.last_stats) >= 0.5 {
                    let (ups, rps, ..) = g.get_stats();
                    log::trace!("UPS: {:?}, RPS: {:?}", ups, rps);
                }

                let loop_state = &mut g.game;
                let settings = &loop_state.settings;
                let settings_hash_before = settings.borrow().get_hash();

                let egui_glow = &mut loop_state.egui_glow;
                let gui = &mut loop_state.gui;
                let window = &mut loop_state.window;
                let mut quit = false;

                let game_runner = &mut loop_state.game_runner;
                for event in extra {
                    egui_glow.on_event(event, window);
                    if let Some(gui_event) = event.to_gui_event() {
                        if let settings::gui::GuiEvent::Keyboard(KeyEvent::Pressed(
                            key_code,
                            keymod,
                        )) = gui_event
                        {
                            use crate::input::keys::KeyCode::*;
                            match key_code {
                                F1 => {
                                    let mut settings = settings.borrow_mut();
                                    settings.last_save_state =
                                        Some(b64.encode(game_runner.state_handler.save()));
                                    settings.save().unwrap();
                                }
                                F2 => {
                                    if let Some(save_state) = &settings.borrow().last_save_state {
                                        if let Ok(buf) = &mut b64.decode(save_state) {
                                            game_runner.state_handler.load(buf);
                                        }
                                    }
                                }
                                key_code => {
                                    if check_and_set_fullscreen(window, keymod, key_code) {
                                        return; // Event consumed
                                    }
                                }
                            }
                        }
                        gui.handle_event(
                            &gui_event,
                            vec![
                                #[cfg(feature = "debug")]
                                &mut loop_state.debug,
                                &mut loop_state.audio,
                                &mut loop_state.input,
                                game_runner.state_handler.get_gui(),
                            ],
                        );
                    }

                    if let sdl2::event::Event::Quit { .. } = event {
                        quit = true;
                    }
                }
                let clipboard = &mut loop_state.clipboard;
                egui_glow.run(window, clipboard, |egui_ctx| {
                    let game_runner = &mut loop_state.game_runner;
                    gui.ui(
                        egui_ctx,
                        &mut vec![
                            #[cfg(feature = "debug")]
                            &mut loop_state.debug,
                            &mut loop_state.audio,
                            &mut loop_state.input,
                            game_runner.state_handler.get_gui(),
                        ],
                        &nes_texture,
                    );
                });

                if settings_hash_before != settings.borrow().get_hash() {
                    log::debug!("Settings saved");
                    settings.borrow().save().unwrap();
                }

                unsafe {
                    use glow::HasContext as _;
                    //gl.clear_color(clear_colour[0], clear_colour[1], clear_colour[2], 1.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                }

                // draw things behind egui here

                egui_glow.paint(window);

                // draw things on top of egui here

                window.gl_swap_window();
                if quit {
                    g.exit();
                }
            },
            events,
        )
    }));

    let handle = |events: Vec<sdl2::event::Event>| -> bool {
        run_frame
            .try_lock()
            .map_or(true, |mut handle| handle(events))
    };

    //Note: this is a workaround for https://stackoverflow.com/a/40693139
    let _event_watch = sdl_context.event().unwrap().add_event_watch(|event| {
        use sdl2::event::{Event, WindowEvent};
        if let Event::Window {
            win_event: WindowEvent::Resized(..) | WindowEvent::SizeChanged(..),
            ..
        } = event
        {
            handle(vec![event]);
        };
    });

    'mainloop: loop {
        let events = events_loop.poll_iter().collect::<Vec<_>>();
        if !handle(events) {
            log::debug!("Game loop ended");
            break 'mainloop;
        }
    }

    Ok(())
}

pub struct LocalGameState {
    nes: NesState,
    frame: i32,
}

impl LocalGameState {
    fn new(nes: NesState) -> Result<Self> {
        Ok(Self { nes, frame: 0 })
    }

    pub fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        self.nes.p1_input = inputs[0].0;
        self.nes.p2_input = inputs[1].0;
        self.nes.run_until_vblank();
        self.frame += 1;
        FPS
    }

    fn save(&self) -> Vec<u8> {
        let mut data = self.nes.save_state();
        data.extend(self.frame.to_le_bytes());
        log::debug!("State saved at frame {:?}", self.frame);
        data
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.frame = i32::from_le_bytes(
            data.split_off(data.len() - std::mem::size_of::<i32>())
                .try_into()
                .unwrap(),
        );
        self.nes.load_state(data);
        log::debug!("State loaded at frame {:?}", self.frame);
    }

    fn consume_samples(&mut self) -> Vec<i16> {
        self.nes.apu.consume_samples()
    }

    fn get_frame(&self) -> &Vec<u16> {
        &self.nes.ppu.screen
    }
}

impl Clone for LocalGameState {
    fn clone(&self) -> Self {
        let data = &mut self.save();
        let mut clone = Self {
            nes: NesState::new(self.nes.mapper.clone()),
            frame: 0,
        };
        clone.load(data);
        clone
    }
}

pub trait StateHandler {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps;
    fn consume_samples(&mut self) -> Vec<i16>;
    fn get_frame(&self) -> Option<&Vec<u16>>;
    fn save(&self) -> Vec<u8>;
    fn load(&mut self, data: &mut Vec<u8>);
    fn get_gui(&mut self) -> &mut dyn GuiComponent;
}

pub struct LocalStateHandler {
    state: LocalGameState,
    gui: EmptyGuiComponent,
}

impl StateHandler for LocalStateHandler {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        self.state.advance(inputs)
    }
    fn consume_samples(&mut self) -> Vec<i16> {
        self.state.consume_samples()
    }
    fn get_frame(&self) -> Option<&Vec<u16>> {
        Some(self.state.get_frame())
    }
    fn save(&self) -> Vec<u8> {
        self.state.save()
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.state.load(data)
    }

    fn get_gui(&mut self) -> &mut dyn GuiComponent {
        &mut self.gui
    }
}

pub struct GameRunner {
    pub state_handler: Box<dyn StateHandler>,
}

impl GameRunner {
    pub fn new(state_handler: Box<dyn StateHandler>) -> Result<Self> {
        Ok(Self { state_handler })
    }
    pub fn advance(&mut self, input: &Input) -> Fps {
        let inputs = [input.inputs.get_joypad(0), input.inputs.get_joypad(1)];

        self.state_handler.advance(inputs)
    }

    pub fn get_frame(&mut self) -> Option<ImageData> {
        if let Some(frame) = self.state_handler.get_frame() {
            let mut image_data = ImageData::Color(ColorImage::new(
                [WIDTH as usize, HEIGHT as usize],
                Color32::BLACK,
            ));
            if let ImageData::Color(color_data) = &mut image_data {
                for (i, pixel) in color_data.pixels.iter_mut().enumerate() {
                    let palette_index = frame[i] as usize * 4;
                    let color = &NTSC_PAL[palette_index..palette_index + 4];
                    *pixel =
                        Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]);
                }
            }
            Some(image_data)
        } else {
            None
        }
    }
}
