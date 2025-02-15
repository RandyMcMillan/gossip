use std::cmp::Ordering;

use super::{widgets, GossipUi, Page};
use eframe::{egui, epaint::PathShape};
use egui::{Context, Ui};
use egui_winit::egui::{vec2, Id, Rect, RichText};
use gossip_lib::{comms::ToOverlordMessage, Relay, GLOBALS};
use nostr_types::RelayUrl;

mod active;
mod coverage;
mod known;
mod mine;

pub const RELAY_URL_PREPOPULATE: &str = "wss://";

pub(super) struct RelayUi {
    /// text of search field
    search: String,
    /// how to sort relay entries
    sort: RelaySorting,
    /// which relays to include in the list
    filter: RelayFilter,
    /// Show hidden relays on/off
    show_hidden: bool,
    /// show details on/off
    show_details: bool,
    /// to edit, add the relay url here
    edit: Option<RelayUrl>,
    /// cache relay list for editing
    edit_relays: Vec<Relay>,
    /// did we just finish editing an entry, add it here
    edit_done: Option<RelayUrl>,
    /// do we still need to scroll to the edit
    edit_needs_scroll: bool,

    /// Add Relay dialog
    add_dialog_step: AddRelayDialogStep,
    new_relay_url: String,

    /// Configure List Menu
    configure_list_menu_active: bool,
}

impl RelayUi {
    pub(super) fn new() -> Self {
        Self {
            search: String::new(),
            sort: RelaySorting::default(),
            filter: RelayFilter::default(),
            show_hidden: false,
            show_details: false,
            edit: None,
            edit_relays: Vec::new(),
            edit_done: None,
            edit_needs_scroll: false,
            add_dialog_step: AddRelayDialogStep::Inactive,
            new_relay_url: RELAY_URL_PREPOPULATE.to_string(),
            configure_list_menu_active: false,
        }
    }
}

#[derive(PartialEq, Default)]
pub(super) enum RelaySorting {
    #[default]
    Rank,
    Name,
    WriteRelays,
    AdvertiseRelays,
    HighestFollowing,
    HighestSuccessRate,
    LowestSuccessRate,
}

impl RelaySorting {
    pub fn get_name(&self) -> &str {
        match self {
            RelaySorting::Rank => "Rank",
            RelaySorting::Name => "Name",
            RelaySorting::WriteRelays => "Write Relays",
            RelaySorting::AdvertiseRelays => "Advertise Relays",
            RelaySorting::HighestFollowing => "Following",
            RelaySorting::HighestSuccessRate => "Success Rate",
            RelaySorting::LowestSuccessRate => "Failure Rate",
        }
    }
}

#[derive(PartialEq, Default)]
pub(super) enum RelayFilter {
    #[default]
    All,
    Write,
    Read,
    Advertise,
    Private,
}

impl RelayFilter {
    pub fn get_name(&self) -> &str {
        match self {
            RelayFilter::All => "All",
            RelayFilter::Write => "Write",
            RelayFilter::Read => "Read",
            RelayFilter::Advertise => "Advertise",
            RelayFilter::Private => "Private",
        }
    }
}

#[derive(PartialEq, Default)]
enum AddRelayDialogStep {
    #[default]
    Inactive,
    Step1UrlEntry,
    Step2AwaitOverlord, // TODO add a configure step once we have overlord connection checking
}

///
/// Show the Relays UI
///
pub(super) fn update(app: &mut GossipUi, ctx: &Context, frame: &mut eframe::Frame, ui: &mut Ui) {
    match app.page {
        Page::RelaysActivityMonitor => active::update(app, ctx, frame, ui),
        Page::RelaysCoverage => coverage::update(app, ctx, frame, ui),
        Page::RelaysMine => mine::update(app, ctx, frame, ui),
        Page::RelaysKnownNetwork => known::update(app, ctx, frame, ui),
        _ => {}
    }
}

pub(super) fn relay_scroll_list(
    app: &mut GossipUi,
    ui: &mut Ui,
    relays: Vec<Relay>,
    id_source: Id,
) {
    let scroll_size = ui.available_size_before_wrap();
    let is_editing = app.relays.edit.is_some();
    let enable_scroll = !is_editing && !egui::ScrollArea::is_scrolling(ui, id_source);

    app.vert_scroll_area()
        .id_source(id_source)
        .enable_scrolling(enable_scroll)
        .show(ui, |ui| {
            let mut pos_last_entry = ui.cursor().left_top();
            let mut has_edit_target = false;

            for db_relay in relays {
                let db_url = db_relay.url.clone();

                // is THIS entry being edited?
                let edit = if let Some(edit_url) = &app.relays.edit {
                    if edit_url == &db_url {
                        has_edit_target = true;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };

                // retrieve an updated copy of this relay when editing
                let db_relay = if has_edit_target {
                    if let Ok(Some(entry)) = GLOBALS.storage.read_relay(&db_url) {
                        entry.clone() // update
                    } else {
                        db_relay // can't update
                    }
                } else {
                    db_relay // don't update
                };

                // get details on this relay
                let (is_connected, reasons) =
                    if let Some(entry) = GLOBALS.connected_relays.get(&db_url) {
                        (
                            true,
                            entry
                                .iter()
                                .map(|rj| {
                                    if rj.reason.persistent() {
                                        rj.reason.to_string()
                                    } else {
                                        format!("[{}]", rj.reason)
                                    }
                                })
                                .collect::<Vec<String>>()
                                .join(", "),
                        )
                    } else {
                        (false, "".into())
                    };

                // get timeout if any
                let timeout_until = GLOBALS
                    .relay_picker
                    .excluded_relays_iter()
                    .find(|p| p.key() == &db_url)
                    .map(|f| *f.value());

                let enabled = edit || !is_editing;
                let mut widget = super::widgets::RelayEntry::new(db_relay, app);
                widget.set_edit(edit);
                widget.set_detail(app.relays.show_details);
                widget.set_enabled(enabled);
                widget.set_connected(is_connected);
                widget.set_timeout(timeout_until);
                widget.set_reasons(reasons);
                if let Some(ref assignment) = GLOBALS.relay_picker.get_relay_assignment(&db_url) {
                    widget.set_user_count(assignment.pubkeys.len());
                }
                let response = ui.add_enabled(enabled, widget.clone());
                if response.clicked() {
                    if !edit {
                        app.relays.edit = Some(db_url);
                        app.relays.edit_needs_scroll = true;
                        has_edit_target = true;
                    } else {
                        app.relays.edit_done = Some(db_url);
                        app.relays.edit = None;
                    }
                } else {
                    if edit && has_edit_target && app.relays.edit_needs_scroll {
                        // on the start of an edit, scroll to the entry (after fixed sorting)
                        response.scroll_to_me(Some(egui::Align::Center));
                        app.relays.edit_needs_scroll = false;
                    } else if Some(db_url) == app.relays.edit_done {
                        // on the end of an edit, scroll to the entry (after sorting has reverted)
                        response.scroll_to_me(Some(egui::Align::Center));
                        app.relays.edit_done = None;
                    }
                }
                pos_last_entry = response.rect.left_top();
            }

            if !has_edit_target && !is_entry_dialog_active(app) {
                // the relay we wanted to edit was not in the list anymore
                // -> release edit modal
                app.relays.edit = None;
            }

            // add enough space to show the last relay entry at the top when editing
            if app.relays.edit.is_some() {
                let desired_size = scroll_size - vec2(0.0, ui.cursor().top() - pos_last_entry.y);
                ui.allocate_exact_size(desired_size, egui::Sense::hover());
            }
        });
}

pub(super) fn is_entry_dialog_active(app: &GossipUi) -> bool {
    app.relays.add_dialog_step != AddRelayDialogStep::Inactive
}

pub(super) fn start_entry_dialog(app: &mut GossipUi) {
    app.relays.add_dialog_step = AddRelayDialogStep::Step1UrlEntry;
}

pub(super) fn stop_entry_dialog(app: &mut GossipUi) {
    app.relays.new_relay_url = RELAY_URL_PREPOPULATE.to_string();
    app.relays.add_dialog_step = AddRelayDialogStep::Inactive;
}

pub(super) fn entry_dialog(ctx: &Context, app: &mut GossipUi) {
    let dlg_size = vec2(ctx.screen_rect().width() * 0.66, 120.0);

    egui::Area::new("hide-background-area")
        .fixed_pos(ctx.screen_rect().left_top())
        .movable(false)
        .interactable(false)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            ui.painter().rect_filled(
                ctx.screen_rect(),
                egui::Rounding::same(0.0),
                egui::Color32::from_rgba_unmultiplied(0x9f, 0x9f, 0x9f, 102),
            );
        });

    let id: Id = "relays-add-dialog".into();
    let mut frame = egui::Frame::popup(&ctx.style());
    let area = egui::Area::new(id)
        .movable(false)
        .interactable(true)
        .order(egui::Order::Foreground)
        .fixed_pos(ctx.screen_rect().center() - vec2(dlg_size.x / 2.0, dlg_size.y));
    area.show_open_close_animation(
        ctx,
        &frame,
        app.relays.add_dialog_step != AddRelayDialogStep::Inactive,
    );
    area.show(ctx, |ui| {
        frame.fill = ui.visuals().extreme_bg_color;
        frame.inner_margin = egui::Margin::symmetric(20.0, 10.0);
        frame.show(ui, |ui| {
            ui.set_min_size(dlg_size);
            ui.set_max_size(dlg_size);

            // ui.max_rect is inner_margin size
            let tr = ui.max_rect().right_top();

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.heading("Add a new relay");
                    let rect = Rect::from_x_y_ranges(tr.x..=tr.x + 10.0, tr.y - 20.0..=tr.y - 10.0);
                    ui.allocate_ui_at_rect(rect, |ui| {
                        if ui
                            .add_sized(rect.size(), super::widgets::NavItem::new("\u{274C}", false))
                            .clicked()
                        {
                            stop_entry_dialog(app);
                        }
                    });
                });

                match app.relays.add_dialog_step {
                    AddRelayDialogStep::Inactive => {}
                    AddRelayDialogStep::Step1UrlEntry => entry_dialog_step1(ui, app),
                    AddRelayDialogStep::Step2AwaitOverlord => entry_dialog_step2(ui, app),
                }
            });
        });
    });
}

fn entry_dialog_step1(ui: &mut Ui, app: &mut GossipUi) {
    ui.add_space(10.0);
    ui.add(egui::Label::new("Enter relay URL:"));
    ui.add_space(10.0);

    // validate relay url (we are validating one UI frame later, shouldn't be an issue)
    let is_url_valid = RelayUrl::try_from_str(&app.relays.new_relay_url).is_ok();

    let edit_response = ui.horizontal(|ui| {
        ui.visuals_mut().widgets.inactive.bg_stroke.width = 1.0;
        ui.visuals_mut().widgets.hovered.bg_stroke.width = 1.0;

        // change frame color to error when url is invalid
        if !is_url_valid {
            ui.visuals_mut().widgets.inactive.bg_stroke.color = ui.visuals().error_fg_color;
            ui.visuals_mut().selection.stroke.color = ui.visuals().error_fg_color;
        }

        ui.add(
            text_edit_line!(app, app.relays.new_relay_url)
                .desired_width(ui.available_width())
                .hint_text("wss://myrelay.com"),
        )
    });

    edit_response.inner.request_focus();

    ui.add_space(10.0);
    ui.allocate_ui_with_layout(
        vec2(edit_response.inner.rect.width(), 30.0),
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.visuals_mut().widgets.inactive.weak_bg_fill = app.theme.accent_color();
                ui.visuals_mut().widgets.hovered.weak_bg_fill = {
                    let mut hsva: egui::ecolor::HsvaGamma = app.theme.accent_color().into();
                    hsva.v *= 0.8;
                    hsva.into()
                };
                ui.spacing_mut().button_padding *= 2.0;
                let text = RichText::new("Check").color(ui.visuals().extreme_bg_color);
                if ui
                    .add_enabled(is_url_valid, egui::Button::new(text))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    if let Ok(url) = RelayUrl::try_from_str(&app.relays.new_relay_url) {
                        let _ = GLOBALS
                            .to_overlord
                            .send(ToOverlordMessage::AddRelay(url.clone()));
                        GLOBALS.status_queue.write().write(format!(
                            "I asked the overlord to add relay {}. Check for it below.",
                            &app.relays.new_relay_url
                        ));

                        // send user to known relays page (where the new entry should show up)
                        app.set_page(Page::RelaysKnownNetwork);
                        // search for the new relay so it shows at the top
                        app.relays.search = url.to_string();
                        // set the new relay to edit mode
                        app.relays.edit = Some(url);
                        app.relays.edit_needs_scroll = true;
                        // reset the filters so it will show
                        app.relays.filter = RelayFilter::All;

                        // go to next step
                        app.relays.add_dialog_step = AddRelayDialogStep::Step2AwaitOverlord;
                        app.relays.new_relay_url = RELAY_URL_PREPOPULATE.to_owned();
                    } else {
                        GLOBALS
                            .status_queue
                            .write()
                            .write("That's not a valid relay URL.".to_owned());
                    }
                }
            });
        },
    );
}

fn entry_dialog_step2(ui: &mut Ui, app: &mut GossipUi) {
    // the new relay has been set as the edit relay
    if let Some(url) = app.relays.edit.clone() {
        ui.add_space(10.0);
        ui.add(egui::Label::new(
            "Relay added and is ready to be configured.",
        ));
        ui.add_space(10.0);

        // if the overlord has added the relay, we are done for now
        if GLOBALS.storage.read_relay(&url).is_ok() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.visuals_mut().widgets.inactive.weak_bg_fill = app.theme.accent_color();
                ui.visuals_mut().widgets.hovered.weak_bg_fill = {
                    let mut hsva: egui::ecolor::HsvaGamma = app.theme.accent_color().into();
                    hsva.v *= 0.8;
                    hsva.into()
                };
                ui.spacing_mut().button_padding *= 2.0;
                let text = RichText::new("Configure").color(ui.visuals().extreme_bg_color);
                if ui
                    .add(egui::Button::new(text))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    stop_entry_dialog(app);
                }
            });
        }
    } else {
        ui.add_space(10.0);
        ui.add(egui::Label::new("Adding relay..."));
        ui.add_space(10.0);

        ui.label("If this takes too long, something went wrong.");
        ui.label("Use the 'X' to close this dialog and abort.");
    }
}

///
/// Draw button with configure popup
///
pub(super) fn configure_list_btn(app: &mut GossipUi, ui: &mut Ui) {
    let (response, painter) = ui.allocate_painter(vec2(20.0, 20.0), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let response = if !app.relays.configure_list_menu_active {
        response.on_hover_text("Configure List View")
    } else {
        response
    };
    let btn_rect = response.rect;
    let color = if response.hovered() {
        app.theme.accent_color()
    } else {
        ui.visuals().text_color()
    };
    let mut mesh = egui::Mesh::with_texture((&app.options_symbol).into());
    mesh.add_rect_with_uv(
        btn_rect.shrink(2.0),
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        color,
    );
    painter.add(egui::Shape::mesh(mesh));

    if response.clicked() {
        app.relays.configure_list_menu_active ^= true;
    }

    let button_center_bottom = response.rect.center_bottom();
    let seen_on_popup_position = button_center_bottom + vec2(-180.0, widgets::DROPDOWN_DISTANCE);

    let id: Id = "configure-list-menu".into();
    let mut frame = egui::Frame::popup(ui.style());
    let area = egui::Area::new(id)
        .movable(false)
        .interactable(true)
        .order(egui::Order::Foreground)
        .fixed_pos(seen_on_popup_position)
        .constrain(true);
    if app.relays.configure_list_menu_active {
        let menuresp = area.show(ui.ctx(), |ui| {
            frame.fill = app.theme.accent_color();
            frame.stroke = egui::Stroke::NONE;
            // frame.shadow = egui::epaint::Shadow::NONE;
            frame.rounding = egui::Rounding::same(5.0);
            frame.inner_margin = egui::Margin::symmetric(20.0, 16.0);
            frame.show(ui, |ui| {
                let path = PathShape::convex_polygon(
                    [
                        button_center_bottom,
                        button_center_bottom
                            + vec2(widgets::DROPDOWN_DISTANCE, widgets::DROPDOWN_DISTANCE),
                        button_center_bottom
                            + vec2(-widgets::DROPDOWN_DISTANCE, widgets::DROPDOWN_DISTANCE),
                    ]
                    .to_vec(),
                    app.theme.accent_color(),
                    egui::Stroke::NONE,
                );
                ui.painter().add(path);
                let size = ui.spacing().interact_size.y * egui::vec2(1.6, 0.8);

                // since we are displaying over an accent color background, load that style
                *ui.style_mut() = app.theme.get_on_accent_style();

                ui.horizontal(|ui| {
                    crate::ui::components::switch_with_size(ui, &mut app.relays.show_details, size);
                    ui.label("Show details");
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    crate::ui::components::switch_with_size(ui, &mut app.relays.show_hidden, size);
                    ui.label("Show hidden relays");
                });
            });
        });
        if menuresp.response.clicked_elsewhere() && !response.clicked() {
            app.relays.configure_list_menu_active = false;
        }
    }
}

///
/// Draw relay sort comboBox
///
pub(super) fn relay_sort_combo(app: &mut GossipUi, ui: &mut Ui) {
    let sort_combo = egui::ComboBox::from_id_source(Id::from("RelaySortCombo"));
    sort_combo
        .width(130.0)
        .selected_text("Sort by ".to_string() + app.relays.sort.get_name())
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut app.relays.sort,
                RelaySorting::Rank,
                RelaySorting::Rank.get_name(),
            );
            ui.selectable_value(
                &mut app.relays.sort,
                RelaySorting::Name,
                RelaySorting::Name.get_name(),
            );
            ui.selectable_value(
                &mut app.relays.sort,
                RelaySorting::HighestFollowing,
                RelaySorting::HighestFollowing.get_name(),
            );
            ui.selectable_value(
                &mut app.relays.sort,
                RelaySorting::HighestSuccessRate,
                RelaySorting::HighestSuccessRate.get_name(),
            );
            ui.selectable_value(
                &mut app.relays.sort,
                RelaySorting::LowestSuccessRate,
                RelaySorting::LowestSuccessRate.get_name(),
            );
            ui.selectable_value(
                &mut app.relays.sort,
                RelaySorting::WriteRelays,
                RelaySorting::WriteRelays.get_name(),
            );
            ui.selectable_value(
                &mut app.relays.sort,
                RelaySorting::AdvertiseRelays,
                RelaySorting::AdvertiseRelays.get_name(),
            );
        });
}

///
/// Draw relay filter comboBox
///
pub(super) fn relay_filter_combo(app: &mut GossipUi, ui: &mut Ui) {
    let filter_combo = egui::ComboBox::from_id_source(Id::from("RelayFilterCombo"));
    filter_combo
        .selected_text(app.relays.filter.get_name())
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut app.relays.filter,
                RelayFilter::All,
                RelayFilter::All.get_name(),
            );
            ui.selectable_value(
                &mut app.relays.filter,
                RelayFilter::Write,
                RelayFilter::Write.get_name(),
            );
            ui.selectable_value(
                &mut app.relays.filter,
                RelayFilter::Read,
                RelayFilter::Read.get_name(),
            );
            ui.selectable_value(
                &mut app.relays.filter,
                RelayFilter::Advertise,
                RelayFilter::Advertise.get_name(),
            );
            ui.selectable_value(
                &mut app.relays.filter,
                RelayFilter::Private,
                RelayFilter::Private.get_name(),
            );
        });
}

///
/// Filter a relay entry
/// - return: true if selected
///
pub(super) fn sort_relay(rui: &RelayUi, a: &Relay, b: &Relay) -> Ordering {
    match rui.sort {
        RelaySorting::Rank => b
            .rank
            .cmp(&a.rank)
            .then(b.usage_bits.cmp(&a.usage_bits))
            .then(a.url.cmp(&b.url)),
        RelaySorting::Name => a.url.host().cmp(&b.url.host()),
        RelaySorting::WriteRelays => b
            .has_usage_bits(Relay::WRITE)
            .cmp(&a.has_usage_bits(Relay::WRITE))
            .then(a.url.cmp(&b.url)),
        RelaySorting::AdvertiseRelays => b
            .has_usage_bits(Relay::ADVERTISE)
            .cmp(&a.has_usage_bits(Relay::ADVERTISE))
            .then(a.url.cmp(&b.url)),
        RelaySorting::HighestFollowing => a.url.cmp(&b.url), // FIXME need following numbers here
        RelaySorting::HighestSuccessRate => b
            .success_rate()
            .total_cmp(&a.success_rate())
            .then(a.url.cmp(&b.url)),
        RelaySorting::LowestSuccessRate => a
            .success_rate()
            .total_cmp(&b.success_rate())
            .then(a.url.cmp(&b.url)),
    }
}

///
/// Filter a relay entry
/// - return: true if selected
///
pub(super) fn filter_relay(rui: &RelayUi, ri: &Relay) -> bool {
    let search = if rui.search.len() > 1 {
        ri.url
            .as_str()
            .to_lowercase()
            .contains(&rui.search.to_lowercase())
    } else {
        true
    };

    let filter = match rui.filter {
        RelayFilter::All => true,
        RelayFilter::Write => ri.has_usage_bits(Relay::WRITE),
        RelayFilter::Read => ri.has_usage_bits(Relay::READ),
        RelayFilter::Advertise => ri.has_usage_bits(Relay::ADVERTISE),
        RelayFilter::Private => !ri.has_usage_bits(Relay::INBOX | Relay::OUTBOX),
    };

    search && filter
}
