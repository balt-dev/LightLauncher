#![feature(trim_prefix_suffix)]
#![warn(clippy::pedantic, clippy::perf)]
//! A lightweight SS14 launcher.

pub mod config;
pub mod db;
pub mod gui;
pub mod models;
pub mod serde;
pub mod staticdata;
pub mod utils;
pub mod worker;

use std::{backtrace::Backtrace, process::ExitCode, sync::LazyLock};

use rfd::MessageDialog;

use crate::staticdata::STATIC_DATA as S;

fn main() -> ExitCode {
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic caught: {info}");
        let trace = Backtrace::force_capture();
        log::error!("traceback:\n{trace}");
        MessageDialog::new()
            .set_title("Fatal error! :(")
            .set_description(format!("A fatal error occurred and the program had to close.\nPlease send your log to the developer!\n\nInfo:\n{info}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::exit(-1);
    }));
    
    LazyLock::force(&S);
    utils::setup_logger();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([500.0, 300.0]),
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
            let trace = Backtrace::force_capture();
            MessageDialog::new()
                .set_title("Fatal error! :(")
                .set_description(format!("A fatal error occurred and the program had to close.\nPlease send your log to the developer!\n\nInfo:\n{err}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
            log::error!("fatal error: {err}");
            log::error!("traceback:\n{trace}");
            ExitCode::FAILURE
        }
    }
}
