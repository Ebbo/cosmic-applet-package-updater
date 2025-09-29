use cosmic_config::{Config, ConfigGet, ConfigSet};
use serde::{Deserialize, Serialize};

use crate::package_manager::PackageManager;

pub const CONFIG_VERSION: u64 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PackageUpdaterConfig {
    pub package_manager: Option<PackageManager>,
    pub check_interval_minutes: u32,
    pub auto_check_on_startup: bool,
    pub include_aur_updates: bool,
    pub show_notifications: bool,
    pub show_update_count: bool,
    pub preferred_terminal: String,
}

impl Default for PackageUpdaterConfig {
    fn default() -> Self {
        Self {
            package_manager: None,
            check_interval_minutes: 60,
            auto_check_on_startup: true,
            include_aur_updates: true,
            show_notifications: true,
            show_update_count: true,
            preferred_terminal: "cosmic-term".to_string(),
        }
    }
}

impl PackageUpdaterConfig {
    pub fn load() -> (Config, Self) {
        let config = Config::new("com.cosmic.PackageUpdater", CONFIG_VERSION).unwrap();
        let config_helper = Self::get_entry(&config).unwrap_or_default();
        (config, config_helper)
    }

    pub fn get_entry(config: &Config) -> Option<Self> {
        config.get("config").ok()
    }

    pub fn set_entry(config: &Config, config_helper: &Self) {
        let _ = config.set("config", config_helper);
    }
}