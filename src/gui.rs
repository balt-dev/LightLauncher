use std::{
    cell::Cell,
    collections::HashMap,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use chrono::{Local, prelude::*};
use egui::{RichText, Sense, TextStyle, Vec2, Widget, Window, collapsing_header::CollapsingState};
use indexmap::IndexSet;
use reqwest::Url;

use crate::{
    config::{self, Config},
    models::*,
    staticdata as s,
    utils::ResultExt,
    worker::{self, FileDialogTarget},
};

macro_rules! ui_try {
    ($expr: expr) => {{
        match $expr {
            Ok(_) => (),
            Err(err) => {
                log::error!("error while drawing UI: {err}");
            }
        }
    }};
}

pub enum TabSelection {
    ServerListing,
    Accounts,
    Config,
}

#[derive(Copy, Clone)]
enum ServerDrawMode {
    Hub,
    Pinned { index: usize },
}

enum ServerOperation {
    TogglePin { url: String },
    MoveUp { index: usize },
    MoveDown { index: usize },
    FetchInfo { url: String },
    Refresh { url: String },
    FetchServer { url: String },
}

pub enum Subwindow {
    AddAccount {
        auth: AuthRequest,
        show_password: bool,
        message: String,
    },
    DirectConnect {
        url: String,
    },
}

pub enum HubStatus {
    Unfetched,
    Fetching,
    Ok(IndexSet<String>),
    Error(anyhow::Error),
}

pub struct App {
    pub server_displays: HashMap<String, ServerDisplay>,
    pub hub_servers: HubStatus,
    pub config: Config,
    pub tab: TabSelection,
    pub subwindow: Option<Subwindow>,
    pub last_refresh: Instant,
    queued_server_operation: Cell<Option<ServerOperation>>,
    search_text: String,
    tag_whitelist: String,
    tag_blacklist: String,
    sync_error: Option<String>,
}

impl eframe::App for App {
    fn auto_save_interval(&self) -> core::time::Duration {
        s::AUTOSAVE_INTERVAL
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        let _ = self.config.save();
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let HubStatus::Unfetched = self.hub_servers {
            self.refresh_servers();
        }
        self.handle_server_op();
        self.refresh_tokens();
        worker::handle(self);

        ctx.request_repaint();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::containers::Panel::bottom("bottom_nav")
            .resizable(false)
            .show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    if ui.button("Servers").clicked() && self.subwindow.is_none() {
                        self.tab = TabSelection::ServerListing
                    }
                    if ui.button("Accounts").clicked() && self.subwindow.is_none() {
                        self.tab = TabSelection::Accounts
                    }
                    if ui.button("Config").clicked() && self.subwindow.is_none() {
                        self.tab = TabSelection::Config
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.heading("Light Launcher ");
                ui.weak(format!("v{}", env!("CARGO_PKG_VERSION")));
            });
            ui.separator();
            match self.tab {
                TabSelection::ServerListing => {
                    self.draw_server_listing(ui);
                }
                TabSelection::Accounts => {
                    self.draw_accounts(ui);
                }
                TabSelection::Config => {
                    self.draw_config(ui);
                }
            }
        });

        self.show_account_subwindow(ui);
        self.show_direct_conn_subwindow(ui);
        self.handle_server_op();
    }
}

impl App {
    pub fn new(config: Config) -> Result<Self, anyhow::Error> {
        Ok(Self {
            server_displays: HashMap::new(),
            hub_servers: HubStatus::Unfetched,
            config,
            tab: TabSelection::ServerListing,
            subwindow: None,
            last_refresh: Instant::now() - Duration::from_secs(1000),
            queued_server_operation: Cell::new(None),
            search_text: String::new(),
            tag_whitelist: String::new(),
            tag_blacklist: String::new(),
            sync_error: None,
        })
    }

    fn draw_server_listing(&mut self, ui: &mut egui::Ui) {
        ui.heading("Servers");
        ui.separator();
        egui::containers::Panel::bottom("server_nav")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Search");
                    egui::TextEdit::singleline(&mut self.search_text)
                        .hint_text("wiz east")
                        .desired_width(ui.available_width())
                        .ui(ui);
                });
                ui.horizontal(|ui| {
                    ui.label("Tag Whitelist");
                    egui::TextEdit::singleline(&mut self.tag_whitelist)
                        .hint_text("lang:en rp:med rp:high")
                        .desired_width(ui.available_width())
                        .ui(ui);
                });
                ui.horizontal(|ui| {
                    ui.label("Tag Blacklist");
                    egui::TextEdit::singleline(&mut self.tag_blacklist)
                        .hint_text("18+ region:luna")
                        .desired_width(ui.available_width())
                        .ui(ui);
                });
                ui.separator();
                egui::MenuBar::new().ui(ui, |ui| {
                    if ui.button("Refresh").clicked() && self.subwindow.is_none() {
                        self.hub_servers = HubStatus::Unfetched;
                    }
                    if ui.button("Direct Connect/Pin").clicked() && self.subwindow.is_none() {
                        self.subwindow = Some(Subwindow::DirectConnect { url: String::new() });
                    }
                    if ui.button("Open Replay").clicked() && self.subwindow.is_none() {
                        // TODO
                    }
                });
            });

        egui::ScrollArea::vertical()
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                let mut any_pinned_drawn = false;
                for i in 0..self.config.pinned_servers.len() {
                    let Some(server) = self.config.pinned_servers.get_index(i) else {
                        continue;
                    };
                    any_pinned_drawn |=
                        self.draw_server(ui, &server.clone(), ServerDrawMode::Pinned { index: i });
                }
                if any_pinned_drawn {
                    ui.separator();
                }
                match &mut self.hub_servers {
                    HubStatus::Unfetched | HubStatus::Fetching => {
                        ui.vertical_centered(|ui| ui.weak("Loading hub server list.."));
                    }
                    HubStatus::Error(err) => {
                        ui.vertical_centered(|ui| {
                            ui.weak(format!("Failed to load hub server list\n{err}"))
                        });
                    }
                    HubStatus::Ok(servers) => {
                        if servers.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.weak("No servers were returned from the configured hub.")
                            });
                            ui.vertical_centered(|ui| ui.weak("Check you're using the right one."));
                        }
                        for server in servers.clone() {
                            if self.config.pinned_servers.contains(&server) {
                                continue;
                            }
                            self.draw_server(ui, &server, ServerDrawMode::Hub);
                        }
                    }
                }
            });
    }

    fn draw_accounts(&mut self, ui: &mut egui::Ui) {
        ui.heading("Accounts");
        ui.separator();
        let mut to_remove = vec![];
        for account in self.config.accounts.values() {
            ui.horizontal(|ui| {
                let expired = account.expire_time < Local::now();
                ui.radio_value(
                    &mut self.config.selected_account,
                    Some(account.user_id.clone()),
                    "",
                );
                if expired {
                    ui.weak(&account.username);
                } else {
                    ui.label(&account.username);
                }
                if ui.button("-").clicked() {
                    to_remove.push(account.user_id.clone());
                }
                ui.weak(&account.user_id);
                if expired {
                    ui.weak("Token expired");
                } else {
                    ui.weak(
                        &account
                            .expire_time
                            .format("Token expires %Y/%m/%d %H:%M:%S")
                            .to_string(),
                    );
                }
            });
        }
        for user_id in to_remove {
            self.config.accounts.remove(&user_id);
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Add an account...").clicked() && self.subwindow.is_none() {
                self.subwindow = Some(Subwindow::AddAccount {
                    auth: Default::default(),
                    show_password: false,
                    message: String::new(),
                });
            }
            if ui.button("Refresh").clicked() && self.subwindow.is_none() {
                self.last_refresh = Instant::now() - s::TOKEN_REFRESH_INTERVAL * 100;
                self.refresh_tokens();
            }
        });
    }

    fn draw_config(&mut self, ui: &mut egui::Ui) {
        ui.heading("Config");
        ui.separator();
        egui::Grid::new("config_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Hub URL:");
                egui::TextEdit::singleline(&mut self.config.hub_url)
                    .hint_text("https://hub.spacestation14.com/api/servers")
                    .desired_width(ui.available_width())
                    .ui(ui);

                ui.shrink_height_to_current();
                ui.end_row();

                ui.label("Engine Manifest URL:");
                egui::TextEdit::singleline(&mut self.config.engine_manifest_url)
                    .hint_text("https://robust-builds.cdn.spacestation14.com/manifest.json")
                    .desired_width(ui.available_width())
                    .ui(ui);

                ui.end_row();

                ui.label("Auth URL:");
                egui::TextEdit::singleline(&mut self.config.auth_url)
                    .hint_text("https://auth.spacestation14.com/api/auth")
                    .desired_width(ui.available_width())
                    .ui(ui);
                ui.end_row();

                ui.label("Path to Robust.Loader:");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let response = ui.button("Browse...");
                    if response.clicked() {
                        worker::start(worker::open_file_dialog(FileDialogTarget::LoaderPath))
                    }
                    egui::TextEdit::singleline(&mut self.config.loader_path)
                        .hint_text(".../path/to/your/Robust.Loader")
                        .desired_width(ui.available_width())
                        .ui(ui);
                });
                ui.end_row();

                let label = ui.label("Background Thread Count:");
                if label.hovered() {
                    label.show_tooltip_text(
                        "\
                        The amount of background threads handling things like web requests.\n\
                        You must restart the launcher before changes to this apply.\
                    ",
                    );
                }
                ui.add(
                    egui::Slider::new(&mut self.config.worker_threads, 1..=16).suffix(" threads"),
                );
                ui.end_row();
            });

        ui.label("Signing Key");
        egui::TextEdit::multiline(&mut self.config.signing_key)
            .font(egui::FontId::monospace(13.0))
            .hint_text(
                RichText::new(
                    "\
                -----BEGIN PUBLIC KEY-----\n\
                ????????\n\
                -----END PUBLIC KEY-----\
            ",
                )
                .monospace(),
            )
            .desired_width(ui.available_width())
            .ui(ui);

        egui::Grid::new("sync_grid").num_columns(2).show(ui, |ui| {
            let response = ui.button("Sync to:");
            if response.clicked() {
                worker::start(worker::sync_signing_key(self.config.key_url.clone()));
            }
            egui::TextEdit::singleline(&mut self.config.key_url)
                .hint_text(config::DEFAULT_KEY_URL)
                .desired_width(ui.available_width())
                .ui(ui);
        });
        if let Some(msg) = &self.sync_error {
            ui.label(format!("Failed to synchronize signing key:\n{msg}"));
            if ui.button("Ok").clicked() {
                self.sync_error = None
            }
        }
    }

    fn show_account_subwindow(&mut self, ui: &mut egui::Ui) {
        let Some(Subwindow::AddAccount {
            auth,
            show_password,
            message,
        }) = &mut self.subwindow
        else {
            return;
        };
        let mut open = true;
        Window::new("Add an account")
            .open(&mut open)
            .collapsible(false)
            .resizable([true, false])
            .show(ui.ctx(), |ui| {
                egui::Grid::new("add_account_grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Username:");
                        egui::TextEdit::singleline(&mut auth.username)
                            .hint_text("johndoe")
                            .desired_width(ui.available_width())
                            .ui(ui);

                        ui.shrink_height_to_current();
                        ui.end_row();

                        ui.label("Password:");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let response = ui.selectable_label(
                                *show_password,
                                if *show_password { "Hide" } else { "Show" },
                            );
                            if response.clicked() {
                                *show_password = !*show_password
                            }
                            egui::TextEdit::singleline(&mut auth.password)
                                .hint_text("fuckingpassword")
                                .desired_width(ui.available_width())
                                .password(!*show_password)
                                .ui(ui);
                        });
                        ui.end_row();

                        ui.label("2FA Code:");
                        egui::TextEdit::singleline(&mut auth.tfa_code)
                            .hint_text("123456")
                            .desired_width(ui.available_width())
                            .ui(ui);
                    });
                ui.vertical_centered(|ui| ui.weak(&*message));

                ui.vertical_centered(|ui| {
                    if egui::Button::new(egui::RichText::new("Log in"))
                        .ui(ui)
                        .clicked()
                    {
                        let Ok(url) = parse_dir_url(&self.config.auth_url) else {
                            *message = "The auth URL is malformed. Check your config.".into();
                            return;
                        };
                        worker::start(worker::login(auth.clone(), url));
                        *message = "Logging in...".into();
                    }
                });
            });
        if !open {
            self.subwindow = None;
        }
    }

    fn show_direct_conn_subwindow(&mut self, ui: &mut egui::Ui) {
        let Some(Subwindow::DirectConnect { .. }) = self.subwindow else {
            return;
        };
        let mut open = true;
        Window::new("Direct Connect / Pin")
            .open(&mut open)
            .collapsible(false)
            .resizable([true, false])
            .show(ui.ctx(), |ui| {
                egui::Grid::new("connect_grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let Some(Subwindow::DirectConnect { url }) = &mut self.subwindow else {
                            return;
                        };
                        ui.label("URL:");
                        egui::TextEdit::singleline(url)
                            .hint_text("ss14://example.com:1212")
                            .desired_width(ui.available_width())
                            .ui(ui);
                    });
                ui.horizontal(|ui| {
                    let Some(Subwindow::DirectConnect { url }) = &self.subwindow else {
                        return;
                    };
                    if ui.button("Connect").clicked() {
                        self.connect_to_server(&url);
                    }
                    if ui.button("Pin").clicked() {
                        self.queue_op(ServerOperation::TogglePin { url: url.clone() });
                        self.subwindow = None;
                    }
                })
            });
        if !open {
            self.subwindow = None;
        }
    }

    fn refresh_tokens(&mut self) {
        if Instant::now() - self.last_refresh < s::TOKEN_REFRESH_INTERVAL {
            return;
        }
        self.last_refresh = Instant::now();
        for account in self.config.accounts.values() {
            let time_until_expiration = account.expire_time - Local::now().fixed_offset();
            if time_until_expiration < s::TOKEN_REFRESH_THRESHOLD {
                let Ok(url) = parse_dir_url(&self.config.auth_url) else {
                    return;
                };
                worker::start(worker::refresh_token(account.user_id.clone(), account.token.clone(), url));
            }
        }
    }

    fn refresh_servers(&mut self) {
        self.hub_servers = HubStatus::Fetching;

        worker::start(worker::fetch_servers(self.config.hub_url.clone()));
    }

    fn draw_server(&self, ui: &mut egui::Ui, url: &str, mode: ServerDrawMode) -> bool {
        let Some(server) = self.get_server_display(url) else {
            return false;
        };
        let id = ui.make_persistent_id(&server.listing.address);
        let mut header_clicked = false;
        let mut header =
            CollapsingState::load_with_default_open(ui.ctx(), id, false).show_header(ui, |ui| {
                let start = ui.cursor();
                if start.top() > ui.viewport_rect().height() {
                    return;
                }
                if start.bottom() < 0.0 {
                    return;
                }
                let resp = {
                    let size: Vec2 = [ui.viewport_rect().width(), start.size().y].into();
                    let mut rect = ui.cursor();
                    rect.set_width(size.x);
                    rect.set_height(size.y);
                    let id = ui.unique_id();
                    let mut response = ui.interact(rect, id, egui::Sense::click());
                    response.set_intrinsic_size(size);
                    response
                };
                header_clicked = resp.clicked();
                ui.columns_const(|[c1, c2]| {
                    let mut rt = RichText::new(server.display_name());
                    if let ServerDrawMode::Pinned { .. } = mode {
                        rt = rt.strong()
                    }
                    c1.add(egui::Label::new(rt).truncate().selectable(false));
                    c2.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_enabled_ui(!matches!(server.status, FetchStatus::Fetching), |ui| {
                            if egui::Button::new(egui::RichText::new("⟳")).ui(ui).clicked() {
                                log::info!("refreshing server {}", server.listing.address);
                                self.queue_op(ServerOperation::Refresh {
                                    url: server.listing.address.clone(),
                                });
                            }
                        });
                        ui.add_enabled_ui(
                            matches!(server.status, FetchStatus::Fetched { .. }),
                            |ui| {
                                if egui::Button::new(egui::RichText::new("Connect"))
                                    .ui(ui)
                                    .clicked()
                                {
                                    self.connect_to_server(&server.listing.address);
                                }
                            },
                        );
                        ui.separator();
                        ui.scope(|ui| {
                            ui.set_width(180.0);
                            let format_pin = |rt: RichText| match mode {
                                ServerDrawMode::Hub => rt.weak(),
                                ServerDrawMode::Pinned { .. } => rt,
                            };
                            match &server.status {
                                crate::models::FetchStatus::Unfetched => {
                                    self.queue_op(ServerOperation::FetchServer {
                                        url: server.listing.address.clone(),
                                    });
                                }
                                crate::models::FetchStatus::Fetching => {
                                    ui.vertical_centered(|ui| {
                                        ui.add(
                                            egui::Label::new(format_pin(RichText::new(
                                                "Fetching status...",
                                            )))
                                            .truncate()
                                            .selectable(false),
                                        );
                                    });
                                }
                                crate::models::FetchStatus::Fetched { status, .. } => {
                                    ui.scope(|ui| {
                                        ui.set_width(70.0);
                                        ui.add(
                                            egui::Label::new(format_pin(RichText::new(format!(
                                                "{} / {}",
                                                status.players,
                                                status.soft_max_players.unwrap_or(0)
                                            ))))
                                            .truncate()
                                            .selectable(false),
                                        )
                                    });
                                    ui.separator();
                                    ui.scope(|ui| {
                                        ui.set_width(70.0);
                                        ui.add(
                                            egui::Label::new(format_pin(RichText::new(
                                                status
                                                    .round_start_time
                                                    .map_or("Lobby".into(), format_round_time),
                                            )))
                                            .truncate()
                                            .selectable(false),
                                        )
                                    });
                                }
                                crate::models::FetchStatus::FetchError(_) => {
                                    ui.vertical_centered(|ui| {
                                        ui.add(
                                            egui::Label::new(format_pin(RichText::new("Offline")))
                                                .truncate()
                                                .selectable(false),
                                        );
                                    });
                                }
                            }
                        });
                        ui.separator();
                    });
                });
            });
        header.set_open(header.is_open() ^ header_clicked);
        let server = &*server;
        header.body(|ui| match &server.status {
            crate::models::FetchStatus::Unfetched | crate::models::FetchStatus::Fetching => {
                ui.add(egui::Label::new(RichText::new("Fetching info...")));
            }
            crate::models::FetchStatus::Fetched { status, info } => {
                self.draw_server_top_bar(ui, &status, mode, &server.listing);
                ui.separator();
                match info {
                    crate::models::InfoStatus::Unfetched => {
                        self.queue_op(ServerOperation::FetchInfo {
                            url: server.listing.address.clone(),
                        });
                        ui.add(egui::Label::new(RichText::new("Fetching info...")));
                    }
                    crate::models::InfoStatus::Fetching => {
                        ui.add(egui::Label::new(RichText::new("Fetching info...")));
                    }
                    crate::models::InfoStatus::Fetched(info) => {
                        self.draw_server_info(ui, &info, &status);
                    }
                    crate::models::InfoStatus::FetchError(err) => {
                        ui.add(egui::Label::new(RichText::new(format!(
                            "Failed to fetch info:\n{err}"
                        ))));
                    }
                }
            }
            crate::models::FetchStatus::FetchError(err) => {
                self.draw_server_buttons(ui, mode, &server.listing);
                ui.add(egui::Label::new(RichText::new(format!(
                    "Failed to connect to server:\n{err}"
                ))));
            }
        });
        true
    }

    fn draw_server_top_bar(
        &self,
        ui: &mut egui::Ui,
        status: &ServerStatus,
        mode: ServerDrawMode,
        listing: &ServerListing,
    ) {
        ui.columns_const(|[c1, c2]| {
            c1.set_max_height(20.0);
            c1.horizontal(|ui| {
                if let Some(id) = status.round_id {
                    ui.add(
                        egui::Label::new(RichText::new(format!("Round #{}", id)).weak())
                            .selectable(false),
                    );
                } else {
                    ui.add(egui::Label::new(RichText::new("In lobby").weak()).selectable(false));
                }
                if let Some(map) = &status.map {
                    ui.separator();
                    ui.add(
                        egui::Label::new(RichText::new(format!("Map: {}", map)).weak())
                            .selectable(false),
                    );
                }
                if let Some(preset) = &status.preset {
                    ui.separator();
                    ui.add(
                        egui::Label::new(RichText::new(format!("Preset: {}", preset)).weak())
                            .selectable(false),
                    );
                }
                if status.panic_bunker {
                    ui.separator();
                    ui.add(
                        egui::Label::new(RichText::new("Panic bunker enabled").weak())
                            .selectable(false),
                    );
                }
            });

            self.draw_server_buttons(c2, mode, listing);
        });
    }

    fn draw_server_info(&self, ui: &mut egui::Ui, info: &ServerInfo, status: &ServerStatus) {
        ui.add(egui::Label::new(RichText::new(
            info.desc
                .as_ref()
                .map(|s| &**s)
                .unwrap_or("No description set."),
        )));

        let mut tag_iter = status.tags.iter();
        if let Some(first_tag) = tag_iter.next() {
            ui.collapsing(RichText::new("Tags").weak(), |ui| {
                ui.horizontal(|ui| {
                    ui.weak(first_tag);
                    for tag in tag_iter {
                        ui.weak(tag);
                    }
                });
            });
        }

        ui.separator();

        // nav buttons
        ui.horizontal(|ui| {
            if let Some(privacy) = &info.privacy_policy
                && let Some(link) = &privacy.link
            {
                if ui.button("Privacy Policy").clicked() {
                    let _ = open::that_detached(link).warn_for_error();
                }
            }
            for link in &info.links {
                let ServerLink {
                    name: Some(name),
                    url: Some(link),
                    ..
                } = link
                else {
                    continue;
                };
                if ui.button(name).clicked() {
                    let _ = open::that_detached(link).warn_for_error();
                }
            }
        });
    }

    fn queue_op(&self, op: ServerOperation) {
        self.queued_server_operation.set(Some(op));
    }

    fn matches_search(&self, server: &ServerDisplay) -> bool {
        // todo: cache the shit outta this

        let words = self.search_text.to_lowercase();
        let mut words = words.split_ascii_whitespace();
        let display_name = server.display_name().to_lowercase();
        if !words.all(|word| display_name.contains(word)) {
            return false;
        }

        if let FetchStatus::Fetched { status, .. } = &server.status {
            let good_tag_list = self.tag_whitelist.trim_ascii().to_lowercase();
            let bad_tag_list = self.tag_blacklist.trim_ascii().to_lowercase();
            let good_tags = good_tag_list.split_ascii_whitespace().collect::<Vec<_>>();
            let bad_tags = bad_tag_list.split_ascii_whitespace().collect::<Vec<_>>();
            if good_tag_list != ""
                && status.tags.iter().all(|haystack| {
                    good_tags.iter().all(|needle| !haystack.contains(needle))
                        || bad_tag_list != ""
                            && status.tags.iter().any(|haystack| {
                                bad_tags.iter().any(|needle| haystack.contains(needle))
                            })
                })
            {
                return false;
            }
        }

        true
    }

    fn handle_server_op(&mut self) {
        let Some(pinop) = self.queued_server_operation.take() else {
            return;
        };
        match pinop {
            ServerOperation::TogglePin { url } => {
                if self.config.pinned_servers.contains(&url) {
                    self.config.pinned_servers.shift_remove(&url);
                } else {
                    if !self.server_displays.contains_key(&url) {
                        self.server_displays.insert(
                            url.clone(),
                            ServerDisplay {
                                listing: ServerListing {
                                    name: None,
                                    address: url.clone(),
                                },
                                status: FetchStatus::Unfetched,
                            },
                        );
                    }
                    self.config.pinned_servers.insert(url);
                }
            }
            ServerOperation::MoveUp { index } => {
                if index == 0 {
                    return;
                }
                self.config.pinned_servers.swap_indices(index, index - 1);
            }
            ServerOperation::MoveDown { index } => {
                if index >= self.config.pinned_servers.len() - 1 {
                    return;
                }
                self.config.pinned_servers.swap_indices(index, index + 1);
            }
            ServerOperation::Refresh { url } => {
                let Some(server) = self.server_displays.get_mut(&url) else {
                    return;
                };
                server.status = FetchStatus::Unfetched
            }
            ServerOperation::FetchServer { url } => {
                let Some(server) = self.server_displays.get_mut(&url) else {
                    return;
                };
                server.status = FetchStatus::Fetching;
                worker::start(worker::fetch_server_data(url));
            }
            ServerOperation::FetchInfo { url } => {
                let Some(ServerDisplay {
                    status: FetchStatus::Fetched { info, .. },
                    ..
                }) = self.server_displays.get_mut(&url)
                else {
                    return;
                };
                *info = InfoStatus::Fetching;
                worker::send(worker::fetch_server_info(url));
            }
        }
        let _ = self.config.save().warn_for_error();
    }

    fn connect_to_server(&self, address: &str) {
        log::info!("connecting to server {}", address);
        // TODO
    }

    fn draw_server_buttons(
        &self,
        ui: &mut egui::Ui,
        mode: ServerDrawMode,
        listing: &ServerListing,
    ) {
        let pinned = self.config.pinned_servers.contains(&listing.address);
        ui.set_max_height(20.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(if pinned { "Unpin" } else { "Pin" }).clicked() {
                self.queue_op(ServerOperation::TogglePin {
                    url: listing.address.clone(),
                })
            }
            match mode {
                ServerDrawMode::Hub => {}
                ServerDrawMode::Pinned { index } => {
                    if ui.button("⏷").clicked() {
                        self.queue_op(ServerOperation::MoveDown { index })
                    }
                    if ui.button("⏶").clicked() {
                        self.queue_op(ServerOperation::MoveUp { index })
                    }
                }
            }
        });
    }

    fn get_server_display(&self, url: &str) -> Option<&ServerDisplay> {
        self.server_displays
            .get(url)
            .filter(|server| self.matches_search(server))
    }

    fn get_server_display_mut(&mut self, url: &str) -> Option<&mut ServerDisplay> {
        let server = self.server_displays.get(url)?;
        if !self.matches_search(server) {
            return None;
        }
        self.server_displays.get_mut(url)
    }
}

fn format_round_time(round_start_time: DateTime<FixedOffset>) -> String {
    let elapsed = Local::now().fixed_offset() - round_start_time;
    format!("{}h {}m", elapsed.num_hours(), elapsed.num_minutes() % 60)
}

fn parse_dir_url(url: &str) -> Result<Url, anyhow::Error> {
    let mut s = url.to_string();
    if !s.ends_with('/') {
        s.push('/');
    }
    Url::parse(&s).map_err(|e| anyhow::anyhow!(e))
}

fn parse_file_url(url: &str) -> Result<Url, anyhow::Error> {
    let mut s = url.to_string();
    if let Some(stripped) = s.strip_suffix('/') {
        s = stripped.to_string();
    }
    Url::parse(&s).map_err(|e| anyhow::anyhow!(e))
}
