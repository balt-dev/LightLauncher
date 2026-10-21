use std::{
    fs::{self, File},
    io::Write,
};

use chrono::Local;
use log::LevelFilter;
use simplelog::{
    ColorChoice, CombinedLogger, Config, ConfigBuilder, SharedLogger, TermLogger, TerminalMode,
    WriteLogger,
};

use crate::{staticdata, utils};

pub fn setup_logger() {
    let now = Local::now().format("%Y-%m-%dT%H-%M-%S");
    let conf = ConfigBuilder::new()
        .add_filter_allow_str("light_launcher")
        .set_time_format_rfc3339()
        .set_location_level(LevelFilter::Error)
        .set_target_level(LevelFilter::Error)
        .set_thread_level(LevelFilter::Error)
        .set_time_format_rfc3339()
        .build();
    let filter = cfg_select! {
        debug_assertions => LevelFilter::Debug,
        _ => LevelFilter::Info,
    };
    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
        filter,
        conf.clone(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )];
    let log_path = staticdata::STATIC_DATA.datapath.join("logs");
    let res = fs::create_dir_all(&log_path)
        .and_then(|_| File::create(log_path.join(format!("{now}.log"))));
    match res {
        Ok(handle) => {
            loggers.push(WriteLogger::new(filter, conf, handle));
            CombinedLogger::init(loggers).expect("failed to initialize loggers :(");
        }
        Err(err) => {
            CombinedLogger::init(loggers).expect("failed to initialize loggers :(");
            log::error!("failed to open log file, logging only to stderr: {err}");
        }
    }
    log::info!("logging initialized");
}
