use std::{collections::HashSet, sync::Arc};

use egui::{
    Align2, Color32, Context, CursorIcon, FontId, Id, RichText, Rounding, Sense, TextStyle, Ui,
    Vec2, Widget, WidgetInfo, WidgetText, WidgetType, Window,
};
use winit::dpi::LogicalSize;

#[derive(Clone)]
struct MenuButtonGroup {
    button_ids: HashSet<Id>,
    focused: Option<Id>,
}
impl MenuButtonGroup {
    fn new() -> Self {
        Self {
            button_ids: HashSet::new(),
            focused: None,
        }
    }
}

// A widget that keeps track of focus between each other.
pub struct MenuButton {
    text: WidgetText,
    sense: Sense,
}
impl MenuButton {
    pub const ACTIVE_COLOR: Color32 = Color32::WHITE;
    pub const INACTIVE_COLOR: Color32 = Color32::from_rgb(96, 96, 96);
    const HOVER_BG: Color32 = Color32::from_rgba_premultiplied(16, 16, 16, 10);
    const GROUP_KEY: &'static str = "MENU_BTN_GROUP_KEY";

    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: Self::ui_text(text, Color32::PLACEHOLDER).into(),
            sense: Sense::click(),
        }
    }

    pub fn ui_text(text: impl Into<String>, color: Color32) -> RichText {
        RichText::new(text)
            .color(color)
            .strong()
            .font(FontId::monospace(30.0))
    }

    pub fn ui_text_small(text: impl Into<String>, color: Color32) -> RichText {
        RichText::new(text)
            .color(color)
            .strong()
            .font(FontId::monospace(15.0))
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
            let actual_focus_id = m.focused();
            let parent_id = ui.id().value();
            let group = m.data.get_temp_mut_or_insert_with(
                Id::new(format!("{}_{}", Self::GROUP_KEY, parent_id)),
                MenuButtonGroup::new,
            );
            let own_id = response.id;

            let fallback_focus_id = group.focused.unwrap_or(own_id);
            group.button_ids.insert(own_id);

            if let Some(focused_id) = actual_focus_id {
                if group.button_ids.contains(&focused_id) {
                    // There is a valid MenuButton focused, lets's update the group with this information.
                    group.focused = Some(focused_id);
                } else {
                    // Something outside of the group is focused. Request the safe fallback to be focused
                    m.request_focus(fallback_focus_id);
                }
            } else {
                // Nothing is focused. Request the safe fallback to be focused
                m.request_focus(fallback_focus_id);
            }
        });

        if ui.is_rect_visible(rect) {
            let text_pos = ui.layout().align_size_within_rect(galley.size(), rect).min;
            response = response.on_hover_cursor(CursorIcon::PointingHand);
            ui.painter().rect_filled(
                rect.expand(5.0),
                Rounding::default(),
                if response.hovered() {
                    Self::HOVER_BG
                } else {
                    Color32::TRANSPARENT
                },
            );
            ui.painter().galley(
                text_pos,
                galley,
                if response.has_focus() {
                    Self::ACTIVE_COLOR
                } else {
                    Self::INACTIVE_COLOR
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
