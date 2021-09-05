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
    pub show_gui: bool
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
            show_gui: true
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
            egui::TopBottomPanel::top("menubar_container").show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    egui::menu::menu(ui, "Windows", |ui| {
                        ui.checkbox(&mut self.cpu_window_open, "CPU");
                    });

                });
            });
            egui::Window::new("CPU").collapsible(false)
            .open(&mut self.cpu_window_open).show(ctx, |ui| {
                ui.label( "===== Registers =====");
                egui::Grid::new("my_grid")
                .num_columns(2)
                .spacing([60.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label("A:");
                    ui.horizontal(|ui| ui.monospace(&format!("0x{:02X}", nes.registers.a)));
                    ui.end_row();

                    ui.label("X:");
                    ui.horizontal(|ui| ui.monospace(&format!("0x{:02X}", nes.registers.x)));
                    ui.end_row();

                    ui.label("Y:");
                    ui.horizontal(|ui| ui.monospace(&format!("0x{:02X}", nes.registers.y)));
                    ui.end_row();

                    ui.label("PC:");
                    ui.horizontal(|ui| ui.monospace(&format!("0x{:04X}", nes.registers.pc)));
                    ui.end_row();

                    ui.label("S:");
                    ui.horizontal(|ui| ui.monospace(&format!("0x{:02X}", nes.registers.s)));
                    ui.end_row();

                    ui.label("F:");
                    
                    ui.horizontal(|ui| ui.monospace("nvdzic"));
                    ui.end_row();
                    
                    ui.label("");
                    
                    ui.horizontal(|ui| ui.monospace(&format!("{}{}{}{}{}{}",
                    if nes.registers.flags.negative            {"n"} else {" "},
                    if nes.registers.flags.overflow            {"v"} else {" "},
                    if nes.registers.flags.decimal             {"d"} else {" "},
                    if nes.registers.flags.zero                {"z"} else {" "},
                    if nes.registers.flags.interrupts_disabled {"i"} else {" "},
                    if nes.registers.flags.carry               {"c"} else {" "})));
                    ui.end_row();
                });

                let scroll_area = ScrollArea::auto_sized();

                ui.label("===== Disassembly =====");
                scroll_area.show(ui, |ui| {
                    ui.vertical(|ui| {
                        let mut data_bytes_to_skip = 0;
                        for i in 0 .. 30 {
                            let pc = nes.registers.pc + (i as u16);
                            let opcode = memory::debug_read_byte(nes, pc);
                            let data1 = memory::debug_read_byte(nes, pc + 1);
                            let data2 = memory::debug_read_byte(nes, pc + 2);
                            let (instruction, data_bytes) = disassemble_instruction(opcode, data1, data2);
                            let mut text_color = Color32::from_rgb(255, 255, 255);
    
                            if data_bytes_to_skip > 0 {
                                text_color = Color32::from_rgb(64, 64, 64);
                                data_bytes_to_skip -= 1;
                            } else {
                                data_bytes_to_skip = data_bytes;
                            }
                            ui.add(Label::new(&format!("0x{:04X} - 0x{:02X}:  {}", pc, opcode, instruction)).monospace().text_color(text_color));
                        }
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