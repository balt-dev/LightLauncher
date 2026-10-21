#![warn(clippy::pedantic, clippy::perf)]
//! A lightweight SS14 launcher.

pub mod models;
pub mod gui;
pub mod staticdata;
pub mod utils;
pub mod config;
pub mod serde;
pub mod db;
pub mod worker;

use std::{process::ExitCode, sync::LazyLock};

use crate::staticdata::STATIC_DATA as S;

fn main() -> ExitCode {
    LazyLock::force(&S);
    utils::setup_logger();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        centered: true,
        persist_window: true,
        persistence_path: Some(S.datapath.join(".egui")),
        ..Default::default()
    };
    let res = eframe::run_native(
        "Light Launcher",
        options,
        Box::new(|cc| {
            gui::App::new()
                .map(|a| Box::new(a) as Box<dyn eframe::App>)
                .map_err(|e| e.into_boxed_dyn_error())
        }),
    );
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            log::error!("The launcher had to be closed because of an error. Please send your log to the developer!");
            log::error!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}