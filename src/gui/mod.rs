use std::sync::Arc;

use egui::{
    Align2, Color32, Context, CursorIcon, FontId, Id, RichText, Sense, TextStyle, Ui, Vec2, Widget,
    WidgetInfo, WidgetText, WidgetType, Window,
};
use winit::dpi::LogicalSize;

// A widget that keeps track of focus between each other.
pub struct MenuButton {
    text: WidgetText,
    sense: Sense,
}
impl MenuButton {
    const ACTIVE_COLOR: Color32 = Color32::WHITE;
    const UNACTIVE_COLOR: Color32 = Color32::from_rgb(96, 96, 96);
    const LAST_FOCUS_KEY: &'static str = "LAST_FOCUS";

    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: RichText::new(text)
                .color(Color32::PLACEHOLDER)
                .strong()
                .font(FontId::monospace(30.0))
                .into(),
            sense: Sense::click(),
        }
    }
}
impl Widget for MenuButton {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let mut desired_size = Vec2::ZERO;
        let galley =
            self.text
                .into_galley(ui, Some(false), ui.available_width(), TextStyle::Button);

        desired_size.x += galley.size().x;
        desired_size.y = desired_size.y.max(galley.size().y);
        let (rect, mut response) = ui.allocate_at_least(desired_size, self.sense);
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, galley.text()));

        ui.memory_mut(|m| {
            if response.hovered() {
                m.request_focus(response.id);
            } else {
                let last_focused_data_key = Id::new(MenuButton::LAST_FOCUS_KEY);
                if let Some(current_focus) = m.focus() {
                    m.data.insert_temp(last_focused_data_key, current_focus);
                } else if let Some(last_focused_id) = m.data.get_temp(last_focused_data_key) {
                    m.request_focus(last_focused_id);
                } else {
                    m.request_focus(response.id);
                }
            }
        });

        if ui.is_rect_visible(rect) {
            let text_pos = ui.layout().align_size_within_rect(galley.size(), rect).min;
            response = response.on_hover_cursor(CursorIcon::PointingHand);

            ui.painter().galley(
                text_pos,
                galley,
                if response.has_focus() {
                    Self::ACTIVE_COLOR
                } else {
                    Self::UNACTIVE_COLOR
                },
            );
        }
        response
    }
}

pub fn centered_window(
    window: &Arc<winit::window::Window>,
    ctx: &Context,
    title: Option<&str>,
    content: impl FnOnce(&mut Ui),
) {
    let size: LogicalSize<f32> = window.inner_size().to_logical(window.scale_factor());

    Window::new(title.unwrap_or(""))
        .title_bar(title.is_some())
        .collapsible(false)
        .resizable(false)
        .movable(false)
        .pivot(Align2::CENTER_CENTER)
        .fixed_pos([size.width / 2.0, size.height / 2.0])
        .show(ctx, content);
}
