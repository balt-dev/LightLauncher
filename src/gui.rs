use std::{collections::HashMap, time::{Duration, Instant}};

use chrono::Local;
use egui::{Widget, Window};
use reqwest::Url;

use crate::{
    config::Config, models::{AuthRequest, RefreshRequest, ServerEntry, ServerListing}, staticdata as s, worker::{FileDialogTarget, WorkerMessage, WorkerThread},
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

pub enum Subwindow {
    AddAccount {
        auth: AuthRequest,
        show_password: bool,
        message: String,
    },
    DirectConnect {
        name: String,
        url: String,
    },
}

pub struct App {
    pub hub_servers: Result<Option<Vec<ServerEntry>>, anyhow::Error>,
    pub worker: WorkerThread,
    pub config: Config,
    pub tab: TabSelection,
    pub subwindow: Option<Subwindow>,
    pub last_refresh: Instant,
}

impl eframe::App for App {
    fn auto_save_interval(&self) -> core::time::Duration {
        s::AUTOSAVE_INTERVAL
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        let _ = self.config.save();
    }

    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(err) = self.worker.unfuck() {
            log::error!("worker thread exploded: {err}");
        }

        self.refresh_tokens();
        self.check_worker_thread();
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
    }
}

impl App {
    pub fn new() -> Result<Self, anyhow::Error> {
        let config = Config::load()?;
        let worker = WorkerThread::init();
        Ok(Self {
            hub_servers: Ok(None),
            worker,
            config,
            tab: TabSelection::ServerListing,
            subwindow: None,
            last_refresh: Instant::now() - Duration::from_secs(1000),
        })
    }

    fn draw_server_listing(&mut self, ui: &mut egui::Ui) {
        for server in &self.config.pinned_servers {
            self.draw_server(ui, server, true);
        }
        match &self.hub_servers {
            Ok(None) => { ui.vertical_centered(|ui| ui.weak("Loading server list..")); },
            Err(err) => { ui.vertical_centered(|ui| ui.weak(format!("Failed to load server list\n{err}"))); },
            Ok(Some(servers)) => {
                for server in servers {
                    self.draw_server(ui, server, false);
                }
            }
        }
        
        egui::containers::Panel::bottom("server_nav")
            .resizable(false)
            .show(ui, |ui| {
                ui.collapsing("Filters", |ui| {
                    
                });
                ui.separator();
                egui::MenuBar::new().ui(ui, |ui| {
                    if ui.button("Refresh").clicked() && self.subwindow.is_none() {
                        self.hub_servers = Ok(None);
                        self.refresh_servers();
                    }
                    if ui.button("Direct Connect/Pin").clicked() && self.subwindow.is_none() {
                        self.subwindow = Some(Subwindow::DirectConnect {
                            name: String::new(),
                            url: String::new(),
                        });
                    }
                    if ui.button("Open Replay").clicked() && self.subwindow.is_none() {
                        // TODO
                    }
                });
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
                        let _ = self.worker.send(WorkerMessage::OpenFileDialog {
                            target: FileDialogTarget::LoaderPath,
                        });
                    }
                    egui::TextEdit::singleline(&mut self.config.loader_path)
                        .hint_text(".../path/to/your/Robust.Loader")
                        .desired_width(ui.available_width())
                        .ui(ui);
                });
                ui.end_row();
            });
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
                        ui_try!(self.worker.send(WorkerMessage::LogIn {
                            auth: auth.clone(),
                            url
                        }));
                        *message = "Logging in...".into();
                    }
                });
            });
        if !open {
            self.subwindow = None;
        }
    }
    
    fn show_direct_conn_subwindow(&mut self, ui: &mut egui::Ui) {
        let Some(Subwindow::DirectConnect {
            name,
            url,
        }) = &mut self.subwindow else { return; };
        let mut open = true;
        Window::new("Direct Connect / Pin")
            .open(&mut open)
            .collapsible(false)
            .resizable([true, false])
            .show(ui.ctx(), |ui| {
                egui::Grid::new("add_account_grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Name:");
                        egui::TextEdit::singleline(name)
                            .hint_text("Foobar Station")
                            .desired_width(ui.available_width())
                            .ui(ui);

                        ui.shrink_height_to_current();
                        ui.end_row();

                        ui.label("URL:");
                        egui::TextEdit::singleline(url)
                            .hint_text("https://example.com:1212")
                            .desired_width(ui.available_width())
                            .ui(ui);
                    });
            });
        if !open {
            self.subwindow = None;
        }
    }

    fn check_worker_thread(&mut self) {
        loop {
            let res = match self.worker.check_for_response() {
                Ok(Some(res)) => res,
                Ok(None) => return,
                Err(err) => {
                    log::error!("failed to get worker thread messages: {err}");
                    return;
                }
            };
            log::debug!("got response: {res:?}");

            match res {
                crate::worker::WorkerResponse::Pong => log::debug!("pong"),
                crate::worker::WorkerResponse::LogInSuccessful { account } => {
                    if let Some(Subwindow::AddAccount { .. }) = self.subwindow {
                        self.subwindow = None;
                    }
                    self.config
                        .accounts
                        .insert(account.user_id.clone(), account);
                }
                crate::worker::WorkerResponse::LogInFailure { error } => {
                    if let Some(Subwindow::AddAccount { message, .. }) = &mut self.subwindow {
                        *message = format!(
                            "Failed: {}",
                            if let Some(status) = error
                                .downcast_ref::<reqwest::Error>()
                                .and_then(|e| e.status())
                            {
                                status.to_string()
                            } else {
                                error.to_string()
                            }
                        );
                    }
                }
                crate::worker::WorkerResponse::RefreshSuccessful { id, refresh } => {
                    let Some(account) = self.config.accounts.get_mut(&id) else {
                        return;
                    };
                    account.expire_time = refresh.expire_time;
                    account.token = refresh.new_token;
                }
                crate::worker::WorkerResponse::RefreshFailure { id, error } => {
                    log::warn!("failed to refresh token for account with ID {id}: {error}");
                }
                crate::worker::WorkerResponse::RefreshExpired { id } => {
                    log::info!("account with ID {id} has an expired token");
                }
                crate::worker::WorkerResponse::FileChosen { path, target } => match target {
                    crate::worker::FileDialogTarget::LoaderPath => self.config.loader_path = path,
                },
                crate::worker::WorkerResponse::Nop => {}
                crate::worker::WorkerResponse::FetchFailure { error } => {
                    self.hub_servers = Err(error)
                },
                crate::worker::WorkerResponse::FetchSuccessful { servers } => {
                    self.hub_servers = Ok(Some(servers))
                },
            }
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
                let _ = self.worker.send(WorkerMessage::RefreshToken {
                    id: account.user_id.clone(),
                    auth: RefreshRequest {
                        token: account.token.clone(),
                    },
                    url,
                });
            }
        }
    }

    fn refresh_servers(&mut self) {
        if let Err(_) = self.hub_servers { return; }
        if let Ok(Some(_)) = self.hub_servers { return; }
        
        let _ = self.worker.send(WorkerMessage::FetchServers {
            hub: self.config.hub_url.clone()
        });
    }
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
