use eframe::egui::{self, Color32, CornerRadius, Stroke, Vec2};

pub const COLOR_OBSIDIAN: Color32 = Color32::from_rgb(5, 10, 23);
pub const COLOR_SIDEBAR: Color32 = Color32::from_rgb(8, 15, 30);
pub const COLOR_CARD_BG: Color32 = Color32::from_rgb(19, 31, 52);
pub const COLOR_CARD_BORDER: Color32 = Color32::from_rgb(65, 88, 120);
pub const COLOR_PS5_BLUE: Color32 = Color32::from_rgb(0, 85, 220);
pub const COLOR_PS5_CYAN: Color32 = Color32::from_rgb(133, 187, 255);
pub const COLOR_TEXT_WHITE: Color32 = Color32::from_rgb(245, 248, 255);
pub const COLOR_TEXT_MUTED: Color32 = Color32::from_rgb(164, 180, 204);
pub const COLOR_SUCCESS: Color32 = Color32::from_rgb(113, 220, 171);
pub const COLOR_WARNING: Color32 = Color32::from_rgb(255, 204, 112);
pub const COLOR_DANGER: Color32 = Color32::from_rgb(255, 136, 151);

pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(COLOR_CARD_BG)
        .corner_radius(16)
        .inner_margin(20)
}

pub fn focus_ring(ui: &egui::Ui, response: &egui::Response, radius: u8) {
    let focused = response.has_focus();
    let visible = focused || response.hovered();
    let amount = ui
        .ctx()
        .animate_bool(response.id.with("focus_animation"), visible);
    if amount > 0.0 {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            CornerRadius::same(
                (f32::from(radius).min(response.rect.height() * 0.5) + 2.0).round() as u8,
            ),
            Stroke::new(
                if focused { 1.5 } else { 1.0 },
                Color32::WHITE.gamma_multiply(amount * if focused { 1.0 } else { 0.45 }),
            ),
            egui::StrokeKind::Middle,
        );
    }
}

pub fn apply_ps5_theme(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(COLOR_TEXT_WHITE);
    visuals.panel_fill = COLOR_OBSIDIAN;
    visuals.window_fill = COLOR_SIDEBAR;
    visuals.faint_bg_color = COLOR_CARD_BG;
    visuals.extreme_bg_color = Color32::from_rgb(10, 20, 38);
    visuals.window_corner_radius = CornerRadius::same(16);
    visuals.menu_corner_radius = CornerRadius::same(12);
    visuals.window_stroke = Stroke::new(1.0, COLOR_CARD_BORDER);
    visuals.widgets.noninteractive.bg_fill = COLOR_CARD_BG;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, COLOR_TEXT_MUTED);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, COLOR_CARD_BORDER);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(34, 49, 73);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, COLOR_TEXT_WHITE);
    visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 68, 100);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.bg_fill = COLOR_PS5_BLUE;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.bg_stroke = Stroke::new(2.0, Color32::WHITE);
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widget.corner_radius = CornerRadius::same(10);
    }
    visuals.selection.bg_fill = COLOR_PS5_BLUE;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.hyperlink_color = COLOR_PS5_CYAN;
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(27, 44, 70);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(43, 66, 101);
    visuals.widgets.active.weak_bg_fill = COLOR_PS5_BLUE;
    ctx.set_visuals(visuals);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.animation_time = 0.16;
        style.spacing.item_spacing = Vec2::new(12.0, 10.0);
        style.spacing.button_padding = Vec2::new(16.0, 9.0);
        style.spacing.interact_size = Vec2::new(40.0, 36.0);
        style.spacing.window_margin = egui::Margin::same(24);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(20.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.0));
    });
}
