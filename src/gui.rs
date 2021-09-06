use egui::{ClippedMesh, FontDefinitions};
use egui_wgpu_backend::{BackendError, RenderPass, ScreenDescriptor};
use egui_winit_platform::{Platform, PlatformDescriptor};
use pixels::{wgpu, PixelsContext};
use std::time::Instant;
use winit::window::Window;

use rusticnes_core::nes::NesState;
use rusticnes_core::opcode_info::disassemble_instruction;
use rusticnes_core::memory;

/// Manages all state required for rendering egui over `Pixels`.
pub(crate) struct Gui {
    // State for egui.
    start_time: Instant,
    platform: Platform,
    screen_descriptor: ScreenDescriptor,
    rpass: RenderPass,
    paint_jobs: Vec<ClippedMesh>,

    // State for the demo app.
    cpu_window_open: bool,
    pub show_gui: bool,
    pub actual_fps: u16
}

use egui::{color::*, *};
impl Gui {
    /// Create egui.
    pub(crate) fn new(width: u32, height: u32, scale_factor: f64, pixels: &pixels::Pixels) -> Self {
        let platform = Platform::new(PlatformDescriptor {
            physical_width: width,
            physical_height: height,
            scale_factor,
            font_definitions: FontDefinitions::default(),
            style: Default::default(),
        });
        let screen_descriptor = ScreenDescriptor {
            physical_width: width,
            physical_height: height,
            scale_factor: scale_factor as f32,
        };
        let rpass = RenderPass::new(pixels.device(), pixels.render_texture_format(), 1);

        Self {
            start_time: Instant::now(),
            platform,
            screen_descriptor,
            rpass,
            paint_jobs: Vec::new(),
            cpu_window_open: true,
            show_gui: true,
            actual_fps: 0
        }
    }

    /// Handle input events from the window manager.
    pub(crate) fn handle_event(&mut self, event: &winit::event::Event<'_, ()>) {
        self.platform.handle_event(event);
    }

    /// Resize egui.
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.screen_descriptor.physical_width = width;
            self.screen_descriptor.physical_height = height;
        }
    }

    /// Update scaling factor.
    pub(crate) fn scale_factor(&mut self, scale_factor: f64) {
        self.screen_descriptor.scale_factor = scale_factor as f32;
    }

    /// Prepare egui.
    pub(crate) fn prepare(&mut self, window: &Window, nes: &NesState) {
        self.platform
            .update_time(self.start_time.elapsed().as_secs_f64());

        // Begin the egui frame.
        self.platform.begin_frame();

        // Draw the demo application.
        self.ui(&self.platform.context(), nes);

        // End the egui frame and create all paint jobs to prepare for rendering.
        let (_output, paint_commands) = self.platform.end_frame(Some(window));
        self.paint_jobs = self.platform.context().tessellate(paint_commands);
    }

    /// Create the UI using egui.
    fn ui(&mut self, ctx: &egui::CtxRef, nes: &NesState) {
        if self.show_gui {
            let window_open = &mut self.cpu_window_open;
            let actual_fps = self.actual_fps;
            egui::TopBottomPanel::top("menubar_container").show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    egui::menu::menu(ui, "Windows", |ui| {
                        ui.checkbox(window_open, "CPU");
                    });
                    ui.with_layout(egui::Layout::right_to_left(), |ui| {
                        ui.monospace(format!("FPS (UI): {:?}", actual_fps));
                        ui.separator();
                    });
                });
            });
            
            egui::Window::new("CPU").collapsible(false)
            .open(&mut self.cpu_window_open).show(ctx, |ui| {
                ui.with_layout(egui::Layout::left_to_right(), |ui| {
                    CollapsingHeader::new("Registers")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("register_grid")
                        .num_columns(2)
                        .spacing([60.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("A:");
                            ui.horizontal(|ui| ui.add(Label::new(&format!("0x{:02X}", nes.registers.a)).monospace().text_color(Color32::WHITE)));
                            ui.end_row();
        
                            ui.label("X:");
                            ui.horizontal(|ui| ui.add(Label::new(&format!("0x{:02X}", nes.registers.x)).monospace().text_color(Color32::WHITE)));
                            ui.end_row();
        
                            ui.label("Y:");
                            ui.horizontal(|ui| ui.add(Label::new(&format!("0x{:02X}", nes.registers.y)).monospace().text_color(Color32::WHITE)));
                            ui.end_row();
        
                            ui.label("PC:");
                            ui.horizontal(|ui| ui.add(Label::new(&format!("0x{:04X}", nes.registers.pc)).monospace().text_color(Color32::WHITE)));
                            ui.end_row();
        
                            ui.label("S:");
                            ui.horizontal(|ui| ui.add(Label::new(&format!("0x{:02X}", nes.registers.s)).monospace().text_color(Color32::WHITE)));
                            ui.end_row();
        
                            ui.label("F:");
                            
                            ui.horizontal(|ui| ui.monospace("nvdzic"));
                            ui.end_row();
                            
                            ui.label("");
                            
                            ui.horizontal(|ui| ui.add(Label::new(&format!("{}{}{}{}{}{}",
                            if nes.registers.flags.negative            {"n"} else {" "},
                            if nes.registers.flags.overflow            {"v"} else {" "},
                            if nes.registers.flags.decimal             {"d"} else {" "},
                            if nes.registers.flags.zero                {"z"} else {" "},
                            if nes.registers.flags.interrupts_disabled {"i"} else {" "},
                            if nes.registers.flags.carry               {"c"} else {" "})).monospace().text_color(Color32::WHITE)));
                            ui.end_row();
                        });
                    });
                    
                    CollapsingHeader::new("Disassembly")
                    .default_open(false)
                    .show(ui, |ui| {
                        let scroll_area = ScrollArea::auto_sized();
                        
                        scroll_area.show(ui, |ui| {
                            egui::Grid::new("register_grid")
                            .num_columns(2)
                            .spacing([10.0, 4.0])
                            .striped(true)
                            .show(ui, |ui| {
                                let mut data_bytes_to_skip = 0;
                                for i in 0 .. 300 {
                                    let pc = nes.registers.pc + (i as u16);
                                    let opcode = memory::debug_read_byte(nes, pc);
                                    let data1 = memory::debug_read_byte(nes, pc + 1);
                                    let data2 = memory::debug_read_byte(nes, pc + 2);
                                    let (instruction, data_bytes) = disassemble_instruction(opcode, data1, data2);
                                    let mut text_color = Color32::WHITE;
                                    
                                    if data_bytes_to_skip > 0 {
                                        text_color = Color32::from_rgb(64, 64, 64);
                                        data_bytes_to_skip -= 1;
                                    } else {
                                        data_bytes_to_skip = data_bytes;
                                    }
                                    ui.add(Label::new(&format!("0x{:04X}", pc)).monospace());
                                    ui.add(Label::new(&format!("{}", instruction)).monospace().text_color(text_color));
                                    ui.end_row();
                                }    
                            });
                        });
                    });    
                });
            });
        }
    }

    /// Render egui.
    pub(crate) fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        render_target: &wgpu::TextureView,
        context: &PixelsContext
    ) -> Result<(), BackendError> {
        // Upload all resources to the GPU.
        self.rpass.update_texture(
            &context.device,
            &context.queue,
            &self.platform.context().texture(),
        );
        self.rpass
            .update_user_textures(&context.device, &context.queue);
        self.rpass.update_buffers(
            &context.device,
            &context.queue,
            &self.paint_jobs,
            &self.screen_descriptor,
        );

        // Record all render passes.
        self.rpass.execute(
            encoder,
            render_target,
            &self.paint_jobs,
            &self.screen_descriptor,
            None,
        )
    }
}