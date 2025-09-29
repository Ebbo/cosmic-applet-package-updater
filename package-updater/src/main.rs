mod app;
mod config;
mod package_manager;

use app::CosmicAppletPackageUpdater;

fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<CosmicAppletPackageUpdater>(())
}