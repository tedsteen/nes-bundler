use std::collections::HashSet;

use egui::{
    Button, Color32, Context, CornerRadius, CursorIcon, FontId, Id, KeyboardShortcut, Label,
    RichText, Role, Sense, TextStyle, TextWrapMode, Ui, Vec2, Widget, WidgetInfo, WidgetText,
};

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

/// Sizes shared by every menu, so they look like one app.
pub mod sizes {
    /// Menu items and status titles ("CONNECTING"), monospace.
    pub const TITLE: f32 = 30.0;
    /// Details under a title ("WAITING FOR SECOND PLAYER"), monospace.
    #[cfg(feature = "netplay")]
    pub const DETAIL: f32 = 15.0;
    /// Action buttons (Close, Cancel, Join...).
    pub const ACTION: f32 = 20.0;
    /// Section headings in Settings.
    pub const HEADING: f32 = 20.0;
    /// Between the rows of a menu.
    pub const ROW_SPACING: f32 = 10.0;
    /// Before the actions at the bottom of a menu, and around the sections of one.
    pub const SECTION_SPACING: f32 = 20.0;
    /// Between a form's labels and its widgets, and between its rows.
    pub const FORM_SPACING: [f32; 2] = [10.0, 6.0];
    /// Inside the menu window.
    pub const MARGIN: i8 = 20;
    /// Dropdowns (audio output, input configurations).
    pub const COMBO_WIDTH: f32 = 160.0;
    /// Paragraphs of text, so they wrap rather than stretch the menu.
    #[cfg(feature = "netplay")]
    pub const TEXT_WIDTH: f32 = 300.0;
}

fn centered_row<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    let inner = ui.vertical_centered(add_contents).inner;
    ui.end_row();
    inner
}

/// A centered title row in a menu, like a menu item that can't be picked.
#[cfg(feature = "netplay")]
pub fn title_ui(ui: &mut Ui, text: impl Into<String>, color: Color32) {
    centered_row(ui, |ui| {
        Label::new(MenuButton::ui_text(text, color))
            .selectable(false)
            .ui(ui);
    });
}

/// A centered row of smaller text, under a title.
#[cfg(feature = "netplay")]
pub fn detail_ui(ui: &mut Ui, text: impl Into<String>) {
    centered_row(ui, |ui| {
        Label::new(
            RichText::new(text)
                .color(MenuButton::ACTIVE_COLOR)
                .strong()
                .font(FontId::monospace(sizes::DETAIL)),
        )
        .selectable(false)
        .ui(ui);
    });
}

/// A button for an action (Close, Cancel...).
pub fn action_button(text: &str) -> Button<'_> {
    Button::new(RichText::new(text).font(FontId::proportional(sizes::ACTION)))
}

/// The actions at the bottom of a menu, centered, set off from what's above.
pub fn actions_ui<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    centered_row(ui, |ui| {
        // Inside the row: egui doesn't allow space added to the grid itself. The grid already
        // puts a row's spacing above.
        ui.add_space(sizes::SECTION_SPACING - sizes::ROW_SPACING);
        ui.spacing_mut().item_spacing.y = sizes::ROW_SPACING;
        add_contents(ui)
    })
}

/// A section of a menu (Audio, Input...), with its heading; sections after the first are set
/// off by a line.
pub fn section_ui(ui: &mut Ui, first: bool, heading: &str, add_contents: impl FnOnce(&mut Ui)) {
    if !first {
        ui.add_space(sizes::SECTION_SPACING / 2.0);
        ui.separator();
        ui.add_space(sizes::SECTION_SPACING / 2.0);
    }
    ui.vertical_centered(|ui| {
        Label::new(RichText::new(heading).font(FontId::proportional(sizes::HEADING)))
            .selectable(false)
            .ui(ui);
    });
    ui.add_space(sizes::ROW_SPACING);
    add_contents(ui);
}

const ESC_SHORTCUT: KeyboardShortcut = egui::KeyboardShortcut {
    modifiers: egui::Modifiers::NONE,
    logical_key: egui::Key::Escape,
};
pub fn esc_pressed(ctx: &Context) -> bool {
    ctx.input_mut(|i| i.consume_shortcut(&ESC_SHORTCUT))
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
            .font(FontId::monospace(sizes::TITLE))
    }
}

impl Widget for MenuButton {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let mut desired_size = Vec2::ZERO;
        // A menu item stays on one line; the menu grows to fit it.
        let galley = self.text.into_galley(
            ui,
            Some(TextWrapMode::Extend),
            f32::INFINITY,
            TextStyle::Button,
        );

        desired_size.x += galley.size().x;
        desired_size.y = desired_size.y.max(galley.size().y);
        let (rect, mut response) = ui.allocate_at_least(desired_size, self.sense);
        response.widget_info(|| WidgetInfo::labeled(Role::Button, true, galley.text()));

        ui.memory_mut(|m| {
            let actual_focus_id = m.focused();
            // One focus group per parent ui.
            let group = m.data.get_temp_mut_or_insert_with(
                ui.scope_id().with(Self::GROUP_KEY),
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
                CornerRadius::default(),
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
