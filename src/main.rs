#![windows_subsystem = "windows"]

use crate::app::App;

mod app;
mod archive;
mod format;
mod status;
mod widget;

fn main() -> iced::Result {
    App::run()
}
