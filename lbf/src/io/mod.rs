use std::fs;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use log::{info, log, Level, LevelFilter};
use svg::Document;

use jagua_rs::io::json_instance::JsonInstance;

use crate::io::json_output::JsonOutput;
use crate::EPOCH;

pub mod cli;
pub mod json_output;
pub mod layout_to_svg;
pub mod svg_export;
pub mod svg_util;

pub fn read_json_instance(path: &Path) -> JsonInstance {
    let file = File::open(path)
        .unwrap_or_else(|err| panic!("could not open instance file: {}, {}", path.display(), err));
    let reader = BufReader::new(file);
    serde_json::from_reader(reader)
        .unwrap_or_else(|err| panic!("could not parse instance file: {}, {}", path.display(), err))
}

pub fn write_json_output(json_output: &JsonOutput, path: &Path) {
    let file = File::create(path)
        .unwrap_or_else(|_| panic!("could not open solution file: {}", path.display()));

    let writer = BufWriter::new(file);

    serde_json::to_writer_pretty(writer, &json_output)
        .unwrap_or_else(|_| panic!("could not write solution file: {}", path.display()));

    info!(
        "Solution JSON written to {:?}",
        fs::canonicalize(path).expect("could not canonicalize path")
    );
}

pub fn write_svg(document: &Document, path: &Path) {
    svg::save(path, document).expect("failed to write svg file");
    info!(
        "Solution SVG written to {:?}",
        fs::canonicalize(&path).expect("could not canonicalize path")
    );
}

pub fn init_logger(level_filter: Option<LevelFilter>) {
    let level_filter = level_filter.unwrap_or(LevelFilter::Info);
    fern::Dispatch::new()
        // Perform allocation-free log formatting
        .format(|out, message, record| {
            let handle = std::thread::current();
            let thread_name = handle.name().unwrap_or("-");

            let duration = EPOCH.elapsed();
            let sec = duration.as_secs() % 60;
            let min = (duration.as_secs() / 60) % 60;
            let hours = (duration.as_secs() / 60) / 60;

            let prefix = format!(
                "[{}] [{:0>2}:{:0>2}:{:0>2}] <{}>",
                record.level(),
                hours,
                min,
                sec,
                thread_name,
            );

            out.finish(format_args!("{:<27}{}", prefix, message))
        })
        // Add blanket level filter -
        .level(level_filter)
        .chain(std::io::stdout())
        .apply()
        .expect("could not initialize logger");
    log!(
        Level::Info,
        "Epoch: {}",
        humantime::format_rfc3339_seconds(std::time::SystemTime::now())
    );
}
