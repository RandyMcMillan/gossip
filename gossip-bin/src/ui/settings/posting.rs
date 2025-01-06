use crate::ui::GossipUi;
use eframe::egui;
use egui::widgets::Slider;
use egui::{Context, TextEdit, Ui};
use gossip_lib::comms::ToOverlordMessage;
use gossip_lib::GLOBALS;

pub(super) fn update(app: &mut GossipUi, _ctx: &Context, _frame: &mut eframe::Frame, ui: &mut Ui) {
    ui.heading("Posting Settings");
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        ui.label("Undo Send (seconds): ")
            .on_hover_text("How many seconds to wait before you can no longer undo the send.");
        ui.add(Slider::new(&mut app.unsaved_settings.undo_send_seconds, 0..=120).text("seconds"));
        reset_button!(app, ui, undo_send_seconds);
    });

    ui.horizontal(|ui| {
        ui.label("Proof of Work: ")
            .on_hover_text("The larger the number, the longer it takes.");
        ui.add(Slider::new(&mut app.unsaved_settings.pow, 0..=40).text("leading zero bits"));
        reset_button!(app, ui, pow);
    });

    ui.horizontal(|ui| {
        ui.checkbox(
            &mut app.unsaved_settings.set_client_tag,
            "Add tag [\"client\",\"gossip\"] to posts",
        )
        .on_hover_text("Takes effect immediately.");
        reset_button!(app, ui, set_client_tag);
    });

    ui.horizontal(|ui| {
        ui.checkbox(
            &mut app.unsaved_settings.set_user_agent,
            format!(
                "Send User-Agent Header to Relays: gossip/{}",
                app.about.version
            ),
        )
        .on_hover_text("Takes effect on next relay connection.");
        reset_button!(app, ui, set_user_agent);
    });

    ui.add_space(20.0);

    ui.horizontal(|ui| {
        ui.label("Blossom servers: ")
            .on_hover_text("Specify your blossom servers (just the host and port if it is not 443). Separate then by spaces or newlines");
        ui.add(
            TextEdit::multiline(
                &mut app.unsaved_settings.blossom_servers)
                .desired_width(f32::INFINITY)
        );
    });

    ui.horizontal(|ui| {
        if ui.button("Publish Blossom Servers").clicked() {
            let _ = GLOBALS
                .to_overlord
                .send(ToOverlordMessage::PushBlossomServers);
        };
    });

    ui.add_space(20.0);
}
