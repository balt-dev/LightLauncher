#![feature(trim_prefix_suffix, once_cell_try)]
#![warn(clippy::pedantic, clippy::perf)]
//! A lightweight SS14 launcher.

pub mod config;
pub mod core;
pub mod db;
pub mod gui;
pub mod models;
pub mod serde;
pub mod staticdata;
pub mod utils;
pub mod worker;

use std::{
    backtrace::Backtrace,
    process::ExitCode,
    sync::{LazyLock, atomic::Ordering},
};

use rfd::MessageDialog;

use crate::{config::Config, db::Database, staticdata::STATIC_DATA as S};

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

    let config_res = Config::load();
    worker::EXECUTOR_THREADS.store(
        config_res.as_ref().map_or(1, |c| c.worker_threads),
        Ordering::Relaxed,
    );

    LazyLock::force(&S);
    Database::init(&S.datapath).expect("failed to initialize database");
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
    let res = config_res.and_then(|config| {
        eframe::run_native(
            "Light Launcher",
            options,
            Box::new(|_cc| {
                gui::App::new(config)
                    .map(|a| Box::new(a) as Box<dyn eframe::App>)
                    .map_err(|e| e.into_boxed_dyn_error())
            }),
        )
        .map_err(|err| anyhow::anyhow!(err))
    });
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
