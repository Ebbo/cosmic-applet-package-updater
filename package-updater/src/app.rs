use cosmic::app::{Core, Task};
use cosmic::cosmic_config::Config;
use cosmic::iced::{time, Subscription, window::Id, Limits};
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window;
use cosmic::widget::{
    button, column, row, text, text_input, toggler, Space, horizontal_space, divider, scrollable
};
use cosmic::Element;
use std::time::{Duration, Instant};
use std::path::PathBuf;

use crate::config::PackageUpdaterConfig;
use crate::package_manager::{PackageManager, PackageManagerDetector, UpdateChecker, UpdateInfo};

pub struct CosmicAppletPackageUpdater {
    core: Core,
    popup: Option<Id>,
    active_tab: PopupTab,
    config: PackageUpdaterConfig,
    config_handler: Config,
    update_info: UpdateInfo,
    last_check: Option<Instant>,
    checking_updates: bool,
    error_message: Option<String>,
    available_package_managers: Vec<PackageManager>,
    ignore_next_sync: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupTab {
    Updates,
    Settings,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    SwitchTab(PopupTab),
    CheckForUpdates,
    DelayedStartupCheck,
    UpdatesChecked(Result<UpdateInfo, String>),
    ConfigChanged(PackageUpdaterConfig),
    LaunchTerminalUpdate,
    TerminalFinished,
    Timer,
    DiscoverPackageManagers,
    SelectPackageManager(PackageManager),
    SetCheckInterval(u32),
    ToggleAutoCheck(bool),
    ToggleIncludeAur(bool),
    ToggleShowNotifications(bool),
    ToggleShowUpdateCount(bool),
    SetPreferredTerminal(String),
    SyncFileChanged,
}

impl cosmic::Application for CosmicAppletPackageUpdater {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.cosmic.PackageUpdater";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let (config_handler, config) = PackageUpdaterConfig::load();
        let available_package_managers = PackageManagerDetector::detect_available();

        let app = Self {
            core,
            popup: None,
            active_tab: PopupTab::Updates,
            config,
            config_handler,
            update_info: UpdateInfo::new(),
            last_check: None,
            checking_updates: false,
            error_message: None,
            available_package_managers,
            ignore_next_sync: true,
        };

        let mut tasks = vec![];

        // Auto-discover package managers on startup if none is configured
        if app.config.package_manager.is_none() {
            tasks.push(Task::done(cosmic::Action::App(Message::DiscoverPackageManagers)));
        }

        // Check for updates on startup if enabled and package manager is available
        if app.config.auto_check_on_startup {
            if app.config.package_manager.is_some() {
                // Add a delay to allow system to stabilize
                tasks.push(Task::perform(
                    async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    },
                    |_| cosmic::Action::App(Message::CheckForUpdates),
                ));
            } else {
                // Delay the update check until after package manager discovery
                tasks.push(Task::done(cosmic::Action::App(Message::DelayedStartupCheck)));
            }
        }

        (app, Task::batch(tasks))
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Self::Message> {
        if self.config.show_update_count {
            // Always show custom button with icon and count (empty string when 0)
            let count_text = if self.update_info.total_updates > 0 {
                format!("{}", self.update_info.total_updates)
            } else {
                String::new()
            };

            let custom_button = button::custom(
                row()
                    .align_y(cosmic::iced::Alignment::Center)
                    .spacing(2)
                    .push(cosmic::widget::icon::from_name(self.get_icon_name()).size(16))
                    .push(text(count_text).size(12))
            )
            .padding([8, 4]) // More top padding to push icon down from panel top
            .class(cosmic::theme::Button::AppletIcon)
            .on_press(Message::TogglePopup);

            if self.update_info.has_updates() {
                cosmic::widget::mouse_area(custom_button)
                    .on_middle_press(Message::LaunchTerminalUpdate)
                    .into()
            } else {
                custom_button.into()
            }
        } else {
            let icon_button = self.core
                .applet
                .icon_button(&self.get_icon_name())
                .on_press(Message::TogglePopup);

            if self.update_info.has_updates() {
                cosmic::widget::mouse_area(icon_button)
                    .on_middle_press(Message::LaunchTerminalUpdate)
                    .into()
            } else {
                icon_button.into()
            }
        }
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let cosmic::cosmic_theme::Spacing { space_s, space_m, .. } = cosmic::theme::active().cosmic().spacing;

        // Tab bar
        let updates_button = button::text(if self.active_tab == PopupTab::Updates {
            "● Updates"
        } else {
            "○ Updates"
        })
        .on_press(Message::SwitchTab(PopupTab::Updates));

        let settings_button = button::text(if self.active_tab == PopupTab::Settings {
            "● Settings"
        } else {
            "○ Settings"
        })
        .on_press(Message::SwitchTab(PopupTab::Settings));

        let tabs = row()
            .width(cosmic::iced::Length::Fill)
            .push(updates_button)
            .push(
                cosmic::widget::container(horizontal_space())
                    .width(cosmic::iced::Length::Fill)
            )
            .push(settings_button);

        // Tab content
        let tab_content = match self.active_tab {
            PopupTab::Updates => self.view_updates_tab(),
            PopupTab::Settings => self.view_settings_tab(),
        };

        // Package illustration - dynamic based on update status
        let (icon_name, emoji) = if self.checking_updates {
            ("view-refresh-symbolic", "⏳")
        } else if self.update_info.has_updates() {
            ("software-update-available-symbolic", "🎁")
        } else {
            ("package-x-generic", "✅")
        };

        let status_text = if self.checking_updates {
            text("Checking...").size(11).align_x(cosmic::iced::Alignment::Center)
        } else if self.update_info.has_updates() {
            text(format!("{} Updates", self.update_info.total_updates)).size(11).align_x(cosmic::iced::Alignment::Center)
        } else {
            text("Up to Date").size(11).align_x(cosmic::iced::Alignment::Center)
        };

        let package_illustration = cosmic::widget::container(
            column()
                .align_x(cosmic::iced::Alignment::Center)
                .spacing(12)
                .push(cosmic::widget::icon::from_name(icon_name).size(48))
                .push(text(emoji).size(28))
                .push(status_text)
        )
        .width(cosmic::iced::Length::Fixed(110.0))
        .height(cosmic::iced::Length::Fixed(150.0))
        .align_x(cosmic::iced::alignment::Horizontal::Center)
        .align_y(cosmic::iced::alignment::Vertical::Center)
        .style(|_theme| cosmic::widget::container::Style {
            background: None,
            ..Default::default()
        })
        .padding(12);

        // Main content area with illustration
        let main_content = row()
            .spacing(space_m)
            .push(
                column()
                    .spacing(space_s)
                    .width(cosmic::iced::Length::Fill)
                    .push(tab_content)
            )
            .push(package_illustration);

        let content = column()
            .spacing(space_s)
            .padding(space_m)
            .push(tabs)
            .push(divider::horizontal::default())
            .push(main_content);

        self.core
            .applet
            .popup_container(content)
            .limits(
                Limits::NONE
                    .min_height(350.0)
                    .max_height(600.0)
                    .min_width(450.0)
                    .max_width(550.0)
            )
            .into()
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::TogglePopup => self.handle_toggle_popup(),
            Message::PopupClosed(id) => self.handle_popup_closed(id),
            Message::SwitchTab(tab) => self.handle_switch_tab(tab),
            Message::CheckForUpdates => {
                if let Some(pm) = self.config.package_manager {
                    self.checking_updates = true;
                    self.error_message = None;
                    let checker = UpdateChecker::new(pm);
                    let include_aur = self.config.include_aur_updates;
                    return Task::perform(
                        async move {
                            checker.check_updates(include_aur).await
                        },
                        |result| cosmic::Action::App(Message::UpdatesChecked(result.map_err(|e| e.to_string()))),
                    );
                }
                Task::none()
            }
            Message::UpdatesChecked(result) => {
                self.checking_updates = false;
                match result {
                    Ok(update_info) => {
                        self.update_info = update_info;
                        self.last_check = Some(Instant::now());
                        self.error_message = None;
                    }
                    Err(error) => {
                        // Handle specific Wayland errors that might occur after system updates
                        if error.contains("Protocol error") || error.contains("wl_surface") {
                            self.error_message = Some("Display system updated. Please restart the applet if issues persist.".to_string());
                        } else {
                            self.error_message = Some(error);
                        }
                    }
                }
                Task::none()
            }
            Message::LaunchTerminalUpdate => {
                if let Some(pm) = self.config.package_manager {
                    let terminal = self.config.preferred_terminal.clone();
                    let command = pm.system_update_command();

                    return Task::perform(
                        async move {
                            // Create a unique marker file to track when the terminal closes
                            let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
                                .unwrap_or_else(|_| "/tmp".to_string());
                            let marker_file = format!("{}/cosmic-package-updater-terminal-{}.marker", runtime_dir, std::process::id());

                            // Create the marker file
                            let _ = std::fs::File::create(&marker_file);

                            // Build command that removes marker file when done
                            let wrapped_command = format!(
                                "{} && echo \"Update completed. Press Enter to exit...\" && read; rm -f \"{}\"",
                                command.replace("\"", "\\\""),
                                marker_file
                            );

                            // Spawn the terminal (it will return immediately due to daemonization)
                            match tokio::process::Command::new(&terminal)
                                .arg("-e")
                                .arg("sh")
                                .arg("-c")
                                .arg(&wrapped_command)
                                .spawn()
                            {
                                Ok(_) => {
                                    // Poll for marker file deletion (terminal closed)
                                    loop {
                                        if !std::path::Path::new(&marker_file).exists() {
                                            break;
                                        }
                                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                    }

                                    // Add a delay to allow system to stabilize after update
                                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                                }
                                Err(_) => {
                                    // Clean up marker file on error
                                    let _ = std::fs::remove_file(&marker_file);
                                }
                            }
                        },
                        |()| cosmic::Action::App(Message::TerminalFinished),
                    );
                }
                Task::none()
            }
            Message::TerminalFinished => {
                // Terminal has finished, trigger update check immediately
                Task::done(cosmic::Action::App(Message::CheckForUpdates))
            }
            Message::ConfigChanged(config) => {
                let old_package_manager = self.config.package_manager;
                self.config = config;
                PackageUpdaterConfig::set_entry(&self.config_handler, &self.config);

                // If package manager was just auto-configured and startup check is enabled,
                // trigger the delayed startup check
                if old_package_manager.is_none() && self.config.package_manager.is_some() && self.config.auto_check_on_startup {
                    Task::done(cosmic::Action::App(Message::DelayedStartupCheck))
                } else {
                    Task::none()
                }
            }
            Message::Timer => {
                // Automatically check for updates if a package manager is configured
                // and we're not already checking
                if !self.checking_updates && self.config.package_manager.is_some() {
                    Task::done(cosmic::Action::App(Message::CheckForUpdates))
                } else {
                    Task::none()
                }
            }
            Message::DiscoverPackageManagers => {
                self.available_package_managers = PackageManagerDetector::detect_available();
                if self.config.package_manager.is_none() {
                    if let Some(preferred) = PackageManagerDetector::get_preferred() {
                        let mut config = self.config.clone();
                        config.package_manager = Some(preferred);
                        return Task::done(cosmic::Action::App(Message::ConfigChanged(config)));
                    }
                }
                Task::none()
            }
            Message::DelayedStartupCheck => {
                // Triggered after package manager discovery to perform startup update check
                if self.config.auto_check_on_startup && self.config.package_manager.is_some() {
                    // Add a delay to allow system to stabilize
                    Task::perform(
                        async move {
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        },
                        |_| cosmic::Action::App(Message::CheckForUpdates),
                    )
                } else {
                    Task::none()
                }
            }
            Message::SelectPackageManager(pm) => {
                let mut config = self.config.clone();
                config.package_manager = Some(pm);
                Task::done(cosmic::Action::App(Message::ConfigChanged(config)))
            }
            Message::SetCheckInterval(interval) => {
                let mut config = self.config.clone();
                config.check_interval_minutes = interval;
                Task::done(cosmic::Action::App(Message::ConfigChanged(config)))
            }
            Message::ToggleAutoCheck(enabled) => {
                let mut config = self.config.clone();
                config.auto_check_on_startup = enabled;
                Task::done(cosmic::Action::App(Message::ConfigChanged(config)))
            }
            Message::ToggleIncludeAur(enabled) => {
                let mut config = self.config.clone();
                config.include_aur_updates = enabled;
                Task::done(cosmic::Action::App(Message::ConfigChanged(config)))
            }
            Message::ToggleShowNotifications(enabled) => {
                let mut config = self.config.clone();
                config.show_notifications = enabled;
                Task::done(cosmic::Action::App(Message::ConfigChanged(config)))
            }
            Message::ToggleShowUpdateCount(enabled) => {
                let mut config = self.config.clone();
                config.show_update_count = enabled;
                Task::done(cosmic::Action::App(Message::ConfigChanged(config)))
            }
            Message::SetPreferredTerminal(terminal) => {
                let mut config = self.config.clone();
                config.preferred_terminal = terminal;
                Task::done(cosmic::Action::App(Message::ConfigChanged(config)))
            }
            Message::SyncFileChanged => {
                // Ignore the first sync event on startup (file creation triggers watcher)
                if self.ignore_next_sync {
                    self.ignore_next_sync = false;
                    return Task::none();
                }

                // Another instance completed an update check, sync our state
                // Only sync if we're not already checking and haven't checked very recently
                if !self.checking_updates && self.config.package_manager.is_some() {
                    let should_sync = self.last_check.map_or(true, |last| {
                        last.elapsed().as_secs() > 3 // Only sync if our last check was more than 3 seconds ago
                    });

                    if should_sync {
                        Task::done(cosmic::Action::App(Message::CheckForUpdates))
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
        }
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let mut subscriptions = vec![];

        // Timer subscription for periodic checks
        if self.config.package_manager.is_some() {
            let timer_subscription = time::every(Duration::from_secs(self.config.check_interval_minutes as u64 * 60))
                .map(|_| Message::Timer);
            subscriptions.push(timer_subscription);

            // File watcher subscription to sync with other instances
            let sync_subscription = Subscription::run_with_id(
                "sync_watcher",
                Self::watch_sync_file()
            );
            subscriptions.push(sync_subscription);
        }

        if subscriptions.is_empty() {
            Subscription::none()
        } else {
            Subscription::batch(subscriptions)
        }
    }
}

impl CosmicAppletPackageUpdater {
    fn get_sync_path() -> PathBuf {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(runtime_dir).join("cosmic-package-updater.sync")
    }

    fn watch_sync_file() -> impl futures::Stream<Item = Message> {
        use notify::{Watcher, RecursiveMode, Event};
        use futures::channel::mpsc;
        use futures::StreamExt;

        async_stream::stream! {
            let sync_path = Self::get_sync_path();

            // Ensure the parent directory exists
            if let Some(parent) = sync_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            // Create the sync file if it doesn't exist
            if !sync_path.exists() {
                let _ = std::fs::File::create(&sync_path);
            }

            let (tx, mut rx) = mpsc::unbounded();

            let mut watcher = match notify::recommended_watcher(move |res: Result<Event, _>| {
                if let Ok(event) = res {
                    if event.kind.is_modify() || event.kind.is_create() {
                        let _ = tx.unbounded_send(());
                    }
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("Failed to create file watcher: {}", e);
                    return;
                }
            };

            if let Err(e) = watcher.watch(&sync_path, RecursiveMode::NonRecursive) {
                eprintln!("Failed to watch sync file: {}", e);
                return;
            }

            while let Some(_) = rx.next().await {
                // Small delay to avoid rapid fire events
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                yield Message::SyncFileChanged;
            }
        }
    }

    fn handle_toggle_popup(&mut self) -> Task<Message> {
        if let Some(p) = self.popup.take() {
            destroy_popup(p)
        } else {
            // Add error handling for popup creation
            if let Some(main_window_id) = self.core.main_window_id() {
                let new_id = Id::unique();
                self.popup.replace(new_id);
                let mut popup_settings = self.core.applet.get_popup_settings(
                    main_window_id,
                    new_id,
                    None,
                    None,
                    None,
                );
                popup_settings.positioner.size_limits = Limits::NONE
                    .max_width(550.0)
                    .min_width(450.0)
                    .min_height(350.0)
                    .max_height(600.0);

                Task::batch(vec![
                    get_popup(popup_settings),
                    window::gain_focus(new_id),
                ])
            } else {
                eprintln!("Failed to get main window ID for popup");
                self.error_message = Some("Unable to open popup window".to_string());
                Task::none()
            }
        }
    }

    fn handle_popup_closed(&mut self, id: Id) -> Task<Message> {
        if self.popup.as_ref() == Some(&id) {
            self.popup = None;
            self.active_tab = PopupTab::Updates;
        }
        Task::none()
    }

    fn handle_switch_tab(&mut self, tab: PopupTab) -> Task<Message> {
        self.active_tab = tab;
        Task::none()
    }

    fn get_icon_name(&self) -> &'static str {
        if self.checking_updates {
            "view-refresh-symbolic"
        } else if self.error_message.is_some() {
            "dialog-error-symbolic"
        } else if self.update_info.has_updates() {
            "software-update-available-symbolic"
        } else {
            "package-x-generic-symbolic"
        }
    }

    fn view_updates_tab(&self) -> Element<'_, Message> {
        let mut widgets = vec![];

        // Status text
        if self.checking_updates {
            widgets.push(text("Checking for updates...").size(18).into());
        } else if let Some(error) = &self.error_message {
            widgets.push(text(format!("Error: {}", error)).size(18).into());
        } else if self.update_info.has_updates() {
            widgets.push(text(format!("{} updates available", self.update_info.total_updates)).size(18).into());

            // Only show package breakdown if package manager supports AUR
            if let Some(pm) = self.config.package_manager {
                if pm.supports_aur() {
                    widgets.push(text(format!("Official packages: {}", self.update_info.official_updates)).into());
                    widgets.push(text(format!("AUR packages: {}", self.update_info.aur_updates)).into());
                }
            }
        } else {
            widgets.push(text("System is up to date").size(18).into());
        }

        // Last check time
        if let Some(last_check) = self.last_check {
            let elapsed = last_check.elapsed();
            let time_text = if elapsed.as_secs() < 60 {
                "Last checked: just now".to_string()
            } else if elapsed.as_secs() < 3600 {
                format!("Last checked: {} minutes ago", elapsed.as_secs() / 60)
            } else {
                format!("Last checked: {} hours ago", elapsed.as_secs() / 3600)
            };
            widgets.push(text(time_text).size(12).into());
        }

        widgets.push(Space::with_height(cosmic::iced::Length::Fixed(16.0)).into());

        // Check button
        widgets.push(
            button::text("Check for Updates")
                .on_press(Message::CheckForUpdates)
                .width(cosmic::iced::Length::Fill)
                .into()
        );

        // Update System button right after Check for Updates if updates available
        if self.update_info.has_updates() {
            widgets.push(
                button::text("Update System")
                    .on_press(Message::LaunchTerminalUpdate)
                    .width(cosmic::iced::Length::Fill)
                    .into()
            );
            widgets.push(text("💡 Tip: Middle-click on the Panel icon").size(10).into());
        }

        if self.update_info.has_updates() {
            widgets.push(Space::with_height(cosmic::iced::Length::Fixed(16.0)).into());

            // Show package list
            widgets.push(text("Packages to update:").size(14).into());
            widgets.push(Space::with_height(cosmic::iced::Length::Fixed(8.0)).into());

            // Create scrollable list of packages
            let mut package_list = column().spacing(4);

            // Group packages by type - only if package manager supports AUR
            let supports_aur = self.config.package_manager
                .map(|pm| pm.supports_aur())
                .unwrap_or(false);

            if supports_aur {
                let official_packages: Vec<_> = self.update_info.packages.iter()
                    .filter(|p| !p.is_aur)
                    .collect();
                let aur_packages: Vec<_> = self.update_info.packages.iter()
                    .filter(|p| p.is_aur)
                    .collect();

                if !official_packages.is_empty() {
                    package_list = package_list.push(text("Official:").size(12));
                    for package in official_packages.iter() {
                        let package_text = if package.current_version != "unknown" {
                            format!("  {} {} → {}", package.name, package.current_version, package.new_version)
                        } else {
                            format!("  {} → {}", package.name, package.new_version)
                        };
                        package_list = package_list.push(text(package_text).size(10));
                    }
                }

                if !aur_packages.is_empty() {
                    if !official_packages.is_empty() {
                        package_list = package_list.push(Space::with_height(cosmic::iced::Length::Fixed(8.0)));
                    }
                    package_list = package_list.push(text("AUR:").size(12));
                    for package in aur_packages.iter() {
                        let package_text = if package.current_version != "unknown" {
                            format!("  {} {} → {}", package.name, package.current_version, package.new_version)
                        } else {
                            format!("  {} → {}", package.name, package.new_version)
                        };
                        package_list = package_list.push(text(package_text).size(10));
                    }
                }
            } else {
                // No AUR support - show all packages without grouping
                for package in self.update_info.packages.iter() {
                    let package_text = if package.current_version != "unknown" {
                        format!("  {} {} → {}", package.name, package.current_version, package.new_version)
                    } else {
                        format!("  {} → {}", package.name, package.new_version)
                    };
                    package_list = package_list.push(text(package_text).size(10));
                }
            }

            // Add the package list in a scrollable styled container
            widgets.push(
                cosmic::widget::container(
                    scrollable(package_list)
                        .width(cosmic::iced::Length::Fill)
                        .height(cosmic::iced::Length::Fixed(100.0)) // Reasonable height with more popup space
                )
                .style(|_theme| cosmic::widget::container::Style {
                    background: Some(cosmic::iced_core::Background::Color([0.1, 0.1, 0.1, 0.1].into())),
                    border: cosmic::iced::Border {
                        radius: cosmic::iced::border::Radius::from(8.0),
                        width: 1.0,
                        color: [0.3, 0.3, 0.3, 0.5].into(),
                    },
                    ..Default::default()
                })
                .padding(12)
                .width(cosmic::iced::Length::Fill)
                .into()
            );

        }

        column()
            .spacing(8)
            .extend(widgets)
            .into()
    }

    fn view_settings_tab(&self) -> Element<'_, Message> {
        let mut widgets = vec![];

        widgets.push(text("Package Manager").size(16).into());

        if self.available_package_managers.is_empty() {
            widgets.push(text("No package managers found").size(14).into());
            widgets.push(
                button::text("Discover Package Managers")
                    .on_press(Message::DiscoverPackageManagers)
                    .into(),
            );
        } else {
            widgets.push(text(format!("Found {} package managers:", self.available_package_managers.len())).size(12).into());
            for &pm in &self.available_package_managers {
                let is_selected = self.config.package_manager == Some(pm);
                let button_text = if is_selected {
                    format!("● {}", pm.name())
                } else {
                    format!("○ {}", pm.name())
                };
                widgets.push(
                    button::text(button_text)
                        .on_press(Message::SelectPackageManager(pm))
                        .width(cosmic::iced::Length::Fill)
                        .into(),
                );
            }
        }

        widgets.push(Space::with_height(cosmic::iced::Length::Fixed(16.0)).into());

        // Check interval
        widgets.push(text("Check Interval (minutes)").size(14).into());
        let interval_value = self.config.check_interval_minutes.to_string();
        widgets.push(
            text_input("60", interval_value)
                .on_input(|s| Message::SetCheckInterval(s.parse::<u32>().unwrap_or(60).max(1).min(1440)))
                .width(cosmic::iced::Length::Fill)
                .into(),
        );

        widgets.push(Space::with_height(cosmic::iced::Length::Fixed(8.0)).into());

        // Toggles
        widgets.push(
            row()
                .spacing(8)
                .align_y(cosmic::iced::Alignment::Center)
                .push(text("Auto-check on startup"))
                .push(Space::with_width(cosmic::iced::Length::Fill))
                .push(toggler(self.config.auto_check_on_startup).on_toggle(Message::ToggleAutoCheck))
                .into(),
        );

        // Only show AUR toggle if package manager supports it
        if let Some(pm) = self.config.package_manager {
            if pm.supports_aur() {
                widgets.push(
                    row()
                        .spacing(8)
                        .align_y(cosmic::iced::Alignment::Center)
                        .push(text("Include AUR updates"))
                        .push(Space::with_width(cosmic::iced::Length::Fill))
                        .push(toggler(self.config.include_aur_updates).on_toggle(Message::ToggleIncludeAur))
                        .into(),
                );
            }
        }

        widgets.push(
            row()
                .spacing(8)
                .align_y(cosmic::iced::Alignment::Center)
                .push(text("Show notifications"))
                .push(Space::with_width(cosmic::iced::Length::Fill))
                .push(toggler(self.config.show_notifications).on_toggle(Message::ToggleShowNotifications))
                .into(),
        );

        widgets.push(
            row()
                .spacing(8)
                .align_y(cosmic::iced::Alignment::Center)
                .push(text("Show update count"))
                .push(Space::with_width(cosmic::iced::Length::Fill))
                .push(toggler(self.config.show_update_count).on_toggle(Message::ToggleShowUpdateCount))
                .into(),
        );

        widgets.push(Space::with_height(cosmic::iced::Length::Fixed(8.0)).into());

        // Terminal setting
        widgets.push(text("Preferred Terminal").size(14).into());
        let terminal_value = if self.config.preferred_terminal.is_empty() {
            "cosmic-term".to_string()
        } else {
            self.config.preferred_terminal.clone()
        };
        widgets.push(
            text_input("cosmic-term", terminal_value)
                .on_input(Message::SetPreferredTerminal)
                .width(cosmic::iced::Length::Fill)
                .into(),
        );

        column()
            .spacing(8)
            .extend(widgets)
            .into()
    }
}