// No console window for the GUI app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    if let Some(code) = plugcam::portable::handle_args() {
        std::process::exit(code);
    }
    plugcam::app::run();
}
