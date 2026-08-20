use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
use tokio::process::Command as TokioCommand;
use std::path::{Path, PathBuf};
use std::fs::{File, OpenOptions};
use std::io::{Write, ErrorKind};

/// Database of installed packages maintained by moss.
const MOSS_SYSTEM_DB: &str = "/.moss/db";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageManager {
    // Arch Linux
    Pacman,
    Paru,
    Yay,
    // Debian/Ubuntu
    Apt,
    // Fedora/RHEL
    Dnf,
    // openSUSE/SUSE
    Zypper,
    // Alpine Linux
    Apk,
    // AerynOS
    Moss,
    // Universal
    Flatpak,
}

impl PackageManager {
    pub fn name(&self) -> &'static str {
        match self {
            PackageManager::Pacman => "pacman",
            PackageManager::Paru => "paru",
            PackageManager::Yay => "yay",
            PackageManager::Apt => "apt",
            PackageManager::Dnf => "dnf",
            PackageManager::Zypper => "zypper",
            PackageManager::Apk => "apk",
            PackageManager::Moss => "moss",
            PackageManager::Flatpak => "flatpak",
        }
    }

    pub fn supports_aur(&self) -> bool {
        matches!(self, PackageManager::Paru | PackageManager::Yay)
    }


    pub fn system_update_command(&self) -> String {
        match self {
            PackageManager::Pacman => "sudo pacman -Syu".to_string(),
            PackageManager::Paru => "paru -Syu".to_string(),
            PackageManager::Yay => "yay -Syu".to_string(),
            PackageManager::Apt => "sudo apt update && sudo apt upgrade".to_string(),
            PackageManager::Dnf => "sudo dnf upgrade".to_string(),
            PackageManager::Zypper => "sudo zypper update".to_string(),
            PackageManager::Apk => "sudo apk upgrade".to_string(),
            // `-u` refreshes the repositories before syncing, so the sync uses
            // the same candidates the applet just reported.
            PackageManager::Moss => "sudo moss sync -u".to_string(),
            PackageManager::Flatpak => "flatpak update".to_string(),
        }
    }
}

impl std::fmt::Display for PackageManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub total_updates: usize,
    pub official_updates: usize,
    pub aur_updates: usize,
    pub packages: Vec<PackageUpdate>,
}

#[derive(Debug, Clone)]
pub struct PackageUpdate {
    pub name: String,
    pub current_version: String,
    pub new_version: String,
    pub is_aur: bool,
}

impl UpdateInfo {
    pub fn new() -> Self {
        Self {
            total_updates: 0,
            official_updates: 0,
            aur_updates: 0,
            packages: Vec::new(),
        }
    }

    pub fn has_updates(&self) -> bool {
        self.total_updates > 0
    }
}

pub struct PackageManagerDetector;

impl PackageManagerDetector {
    pub fn detect_available() -> Vec<PackageManager> {
        let mut available = Vec::new();

        // Check in order of preference
        for pm in [
            // AUR helpers first (most feature-rich for Arch)
            PackageManager::Paru,
            PackageManager::Yay,
            // System package managers
            PackageManager::Pacman,
            PackageManager::Apt,
            PackageManager::Dnf,
            PackageManager::Zypper,
            PackageManager::Apk,
            PackageManager::Moss,
            // Universal package managers
            PackageManager::Flatpak,
        ] {
            if Self::is_available(pm) {
                available.push(pm);
            }
        }

        available
    }

    pub fn get_preferred() -> Option<PackageManager> {
        Self::detect_available().into_iter().next()
    }

    fn is_available(pm: PackageManager) -> bool {
        Command::new("which")
            .arg(pm.name())
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

pub struct UpdateChecker {
    package_manager: PackageManager,
}

impl UpdateChecker {
    pub fn new(package_manager: PackageManager) -> Self {
        Self { package_manager }
    }

    fn get_lock_path() -> PathBuf {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(runtime_dir).join("cosmic-package-updater.lock")
    }

    fn get_sync_path() -> PathBuf {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(runtime_dir).join("cosmic-package-updater.sync")
    }

    fn notify_check_completed() {
        // Touch the sync file to notify other instances
        let sync_path = Self::get_sync_path();
        if let Ok(mut file) = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&sync_path)
        {
            let _ = writeln!(file, "{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs());
        }
    }

    async fn acquire_lock() -> Result<File> {
        let lock_path = Self::get_lock_path();

        // Try to open the lock file exclusively
        match OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                // Write our PID to the lock file
                let _ = writeln!(file, "{}", std::process::id());
                Ok(file)
            }
            Err(e) if e.kind() == ErrorKind::PermissionDenied => {
                Err(anyhow!("Another instance is checking for updates"))
            }
            Err(e) => Err(anyhow!("Failed to acquire lock: {}", e)),
        }
    }

    pub async fn check_updates(&self, include_aur: bool) -> Result<UpdateInfo> {
        // Try to acquire lock first
        let _lock = match Self::acquire_lock().await {
            Ok(lock) => lock,
            Err(e) => {
                eprintln!("Could not acquire lock: {}. Waiting and retrying...", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                // Retry once
                match Self::acquire_lock().await {
                    Ok(lock) => lock,
                    Err(e) => return Err(anyhow!("Update check already in progress: {}", e)),
                }
            }
        };

        let mut update_info = UpdateInfo::new();

        // Step 1: Check official updates first and wait for completion
        match self.check_official_updates().await {
            Ok(official_updates) => {
                let count = official_updates.len();
                update_info.official_updates = count;
                update_info.packages.extend(official_updates);
            }
            Err(e) => {
                eprintln!("Failed to check official updates: {}", e);
                // Retry once after a delay
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                match self.check_official_updates().await {
                    Ok(official_updates) => {
                        let count = official_updates.len();
                        update_info.official_updates = count;
                        update_info.packages.extend(official_updates);
                    }
                    Err(e) => {
                        eprintln!("Retry failed for official updates: {}", e);
                        // Continue with AUR check even if official fails
                    }
                }
            }
        }

        // Step 2: Only after official check is done, check AUR updates if enabled
        if include_aur && self.package_manager.supports_aur() {
            match self.check_aur_updates().await {
                Ok(aur_updates) => {
                    let count = aur_updates.len();
                    update_info.aur_updates = count;
                    update_info.packages.extend(aur_updates);
                }
                Err(e) => {
                    eprintln!("Failed to check AUR updates: {}", e);
                    // Retry once after a delay
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    match self.check_aur_updates().await {
                        Ok(aur_updates) => {
                            let count = aur_updates.len();
                            update_info.aur_updates = count;
                            update_info.packages.extend(aur_updates);
                        }
                        Err(e) => {
                            eprintln!("Retry failed for AUR updates: {}", e);
                            // Continue even if AUR check fails
                        }
                    }
                }
            }
        }

        // Step 3: Calculate final total only after both checks are complete
        update_info.total_updates = update_info.packages.len();

        // Notify other instances that we completed a check
        Self::notify_check_completed();

        // Lock is automatically released when _lock is dropped
        Ok(update_info)
    }

    async fn check_official_updates(&self) -> Result<Vec<PackageUpdate>> {
        // moss only reports candidates that are already in its repository index,
        // so refresh a private index first. This needs no privileges, unlike
        // refreshing the system index.
        if matches!(self.package_manager, PackageManager::Moss) {
            if let Some(root) = Self::refreshed_moss_root().await {
                return self
                    .parse_update_output("moss", vec!["-D", &root, "list", "sync"], false)
                    .await;
            }
        }

        let (cmd, args) = match self.package_manager {
            // Arch-based systems
            PackageManager::Pacman | PackageManager::Paru | PackageManager::Yay => {
                ("checkupdates", vec![])
            }
            // Debian/Ubuntu
            PackageManager::Apt => {
                ("apt", vec!["list", "--upgradable"])
            }
            // Fedora/RHEL
            PackageManager::Dnf => {
                ("dnf", vec!["check-update", "-q"])
            }
            // openSUSE/SUSE
            PackageManager::Zypper => {
                ("zypper", vec!["list-updates"])
            }
            // Alpine Linux
            PackageManager::Apk => {
                ("apk", vec!["-u", "list"])
            }
            // AerynOS: only reached when the private index above is unavailable,
            // in which case the system index is the best we can report against.
            PackageManager::Moss => {
                ("moss", vec!["list", "sync"])
            }
            // Flatpak
            PackageManager::Flatpak => {
                ("flatpak", vec!["remote-ls", "--updates"])
            }
        };

        self.parse_update_output(cmd, args, false).await
    }

    async fn check_aur_updates(&self) -> Result<Vec<PackageUpdate>> {
        let (cmd, args) = match self.package_manager {
            PackageManager::Pacman => return Ok(Vec::new()),
            PackageManager::Paru => ("paru", vec!["-Qu", "--aur"]),
            PackageManager::Yay => ("yay", vec!["-Qu", "--aur"]),
            // Other package managers don't have AUR support
            _ => return Ok(Vec::new()),
        };

        self.parse_update_output(cmd, args, true).await
    }

    /// Prepare a private moss root and refresh its repository index.
    ///
    /// Refreshing the index in the system moss root requires write access as
    /// root, which an applet must not have. Instead we assemble a root of our
    /// own that symlinks the system's installed-package database, active state
    /// and repository configuration, but keeps its own copy of the repository
    /// index. `moss -D <root> repo update` then refreshes that private copy
    /// without touching the system and without taking the system moss lock, so
    /// a background check can never block an install the user is running.
    /// This mirrors what `checkupdates` does for pacman.
    ///
    /// Returns `None` when the private root is unusable, so the caller can fall
    /// back to querying the system index directly.
    async fn refreshed_moss_root() -> Option<String> {
        let root = match Self::prepare_moss_check_root() {
            Ok(root) => root,
            Err(e) => {
                eprintln!("Could not prepare the moss update-check root: {e}");
                return None;
            }
        };

        match TokioCommand::new("moss")
            .args(["-D", &root, "repo", "update"])
            .output()
            .await
        {
            Ok(output) if output.status.success() => Some(root),
            Ok(output) => {
                eprintln!(
                    "moss repo update failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                // An index from an earlier refresh is still worth reporting on.
                Self::moss_root_has_index(&root).then_some(root)
            }
            Err(e) => {
                eprintln!("Could not run moss repo update: {e}");
                Self::moss_root_has_index(&root).then_some(root)
            }
        }
    }

    /// Create the private moss root described by [`Self::refreshed_moss_root`]
    /// and return its path.
    fn prepare_moss_check_root() -> Result<String> {
        // Without the system database there are no installed packages to
        // compare the repository candidates against.
        if !Path::new(MOSS_SYSTEM_DB).is_dir() {
            return Err(anyhow!("{} does not exist", MOSS_SYSTEM_DB));
        }

        let cache_dir = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
            .ok_or_else(|| anyhow!("Neither XDG_CACHE_HOME nor HOME is set"))?;
        let root = cache_dir.join("cosmic-package-updater").join("moss-root");

        std::fs::create_dir_all(root.join(".moss"))?;
        std::fs::create_dir_all(root.join("etc"))?;

        // Installed packages, the active state id and the repository
        // configuration are all read straight from the running system.
        Self::link_into_check_root(MOSS_SYSTEM_DB, &root.join(".moss").join("db"))?;
        Self::link_into_check_root("/usr", &root.join("usr"))?;
        Self::link_into_check_root("/etc/moss", &root.join("etc").join("moss"))?;

        root.into_os_string()
            .into_string()
            .map_err(|path| anyhow!("Cache path is not valid UTF-8: {:?}", path))
    }

    /// Point `link` at `target`, replacing whatever is currently in the way.
    fn link_into_check_root(target: &str, link: &Path) -> Result<()> {
        if let Ok(metadata) = std::fs::symlink_metadata(link) {
            if metadata.is_symlink()
                && std::fs::read_link(link).ok().as_deref() == Some(Path::new(target))
            {
                return Ok(());
            }

            // Anything else here was left behind by a previous run inside our
            // own cache directory, so replacing it is safe.
            if metadata.is_dir() {
                std::fs::remove_dir_all(link)?;
            } else {
                std::fs::remove_file(link)?;
            }
        }

        std::os::unix::fs::symlink(target, link)?;
        Ok(())
    }

    /// Whether the private root holds a repository index from an earlier refresh.
    fn moss_root_has_index(root: &str) -> bool {
        Path::new(root)
            .join(".moss")
            .join("repo")
            .read_dir()
            .is_ok_and(|mut entries| entries.next().is_some())
    }

    async fn parse_update_output(&self, cmd: &str, args: Vec<&str>, is_aur: bool) -> Result<Vec<PackageUpdate>> {
        let output = TokioCommand::new(cmd)
            .args(&args)
            .output()
            .await?;

        if !output.status.success() {
            let exit_code = output.status.code().unwrap_or(-1);

            // Handle exit codes more carefully
            // checkupdates returns 2 when no updates are available
            // paru/yay return 1 when no updates are available
            // dnf returns 100 when updates are available, 0 when no updates
            // apt returns non-zero on error but we check stdout
            if (cmd == "checkupdates" && exit_code == 2) ||
               ((cmd == "paru" || cmd == "yay") && exit_code == 1) ||
               (cmd == "dnf" && exit_code == 100) {
                // No updates available or special success case
                if cmd == "dnf" && exit_code == 100 {
                    // dnf exit code 100 means updates ARE available, continue parsing
                } else {
                    return Ok(Vec::new());
                }
            } else {
                // Any other exit code might still have valid output for some package managers
                // Check if we have stdout output before failing
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim().is_empty() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("Update check failed with exit code {}: {}", exit_code, stderr);
                    return Err(anyhow!("Failed to check for updates (exit {}): {}", exit_code, stderr));
                }
                // Otherwise continue to parse the output
            }
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = Vec::new();

        for line in stdout.lines() {
            if let Some(package) = self.parse_package_line(line, is_aur) {
                packages.push(package);
            }
        }

        Ok(packages)
    }

    fn parse_package_line(&self, line: &str, is_aur: bool) -> Option<PackageUpdate> {
        // Skip header lines
        if line.starts_with("Listing...") || line.starts_with("Done") ||
           line.starts_with("WARNING:") || line.starts_with("S |") ||
           line.starts_with("--+") || line.trim().is_empty() {
            return None;
        }

        match self.package_manager {
            // Arch-based: "package 1.0.0-1 -> 1.0.1-1" or "package 1.0.1-1"
            PackageManager::Pacman | PackageManager::Paru | PackageManager::Yay => {
                if line.contains(" -> ") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 4 && parts[2] == "->" {
                        return Some(PackageUpdate {
                            name: parts[0].to_string(),
                            current_version: parts[1].to_string(),
                            new_version: parts[3].to_string(),
                            is_aur,
                        });
                    }
                } else {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        return Some(PackageUpdate {
                            name: parts[0].to_string(),
                            current_version: "unknown".to_string(),
                            new_version: parts[1].to_string(),
                            is_aur,
                        });
                    }
                }
            }

            // APT: "package/suite version arch [upgradable from: old-version]"
            PackageManager::Apt => {
                if line.contains("[upgradable from:") {
                    // Split by '/' to get package name
                    let name = line.split('/').next()?.to_string();

                    // Extract new version (between '/' and architecture)
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    let new_version = if parts.len() >= 2 {
                        parts[1].to_string()
                    } else {
                        "unknown".to_string()
                    };

                    // Extract old version from [upgradable from: X]
                    let current_version = if let Some(from_idx) = line.find("[upgradable from: ") {
                        let start = from_idx + "[upgradable from: ".len();
                        if let Some(end_idx) = line[start..].find(']') {
                            line[start..start + end_idx].to_string()
                        } else {
                            "unknown".to_string()
                        }
                    } else {
                        "unknown".to_string()
                    };

                    return Some(PackageUpdate {
                        name,
                        current_version,
                        new_version,
                        is_aur: false,
                    });
                }
            }

            // DNF: "package.arch version repo" (3 columns)
            PackageManager::Dnf => {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    // First part is "package.arch"
                    let name = parts[0].split('.').next()?.to_string();
                    let new_version = parts[1].to_string();

                    return Some(PackageUpdate {
                        name,
                        current_version: "unknown".to_string(),
                        new_version,
                        is_aur: false,
                    });
                }
            }

            // Zypper: table format with columns
            // Skip status column and parse name and version
            PackageManager::Zypper => {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 4 {
                    let name = parts[1].trim().to_string();
                    let new_version = parts[3].trim().to_string();

                    return Some(PackageUpdate {
                        name,
                        current_version: "unknown".to_string(),
                        new_version,
                        is_aur: false,
                    });
                }
            }

            // APK: "package-version [upgradable from: old-version]"
            PackageManager::Apk => {
                if line.contains("[upgradable from:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 1 {
                        // First part contains package-version, need to extract package name
                        let pkg_info = parts[0];
                        let name = if let Some(dash_idx) = pkg_info.rfind('-') {
                            pkg_info[..dash_idx].to_string()
                        } else {
                            pkg_info.to_string()
                        };

                        // Extract versions
                        let new_version = parts.get(1).unwrap_or(&"unknown").to_string();

                        let current_version = if let Some(from_idx) = line.find("[upgradable from: ") {
                            let start = from_idx + "[upgradable from: ".len();
                            if let Some(end_idx) = line[start..].find(']') {
                                line[start..start + end_idx].to_string()
                            } else {
                                "unknown".to_string()
                            }
                        } else {
                            "unknown".to_string()
                        };

                        return Some(PackageUpdate {
                            name,
                            current_version,
                            new_version,
                            is_aur: false,
                        });
                    }
                }
            }

            // moss: "name    1.2.3-4 => 1.2.5-6 - Package summary"
            // Only packages with a sync candidate carry the " => " marker, and
            // `moss list sync` prints nothing else.
            PackageManager::Moss => {
                let line = strip_ansi(line);
                let (installed, candidate) = line.split_once(" => ")?;

                let mut installed = installed.split_whitespace();
                let name = installed.next()?.to_string();
                // The version column is right aligned, so the version is the
                // last field of the installed half.
                let current_version = installed.next_back()?.to_string();
                let new_version = candidate.split_whitespace().next()?.to_string();

                return Some(PackageUpdate {
                    name,
                    current_version,
                    new_version,
                    is_aur: false,
                });
            }

            // Flatpak: "name\tapp-id\tversion\tbranch\tremote"
            PackageManager::Flatpak => {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 3 {
                    let name = parts[0].to_string();
                    let new_version = parts[2].to_string();

                    return Some(PackageUpdate {
                        name,
                        current_version: "unknown".to_string(),
                        new_version,
                        is_aur: false,
                    });
                }
            }
        }

        None
    }

}

/// Remove ANSI escape sequences from a line of package manager output.
///
/// moss only colourises when it is writing to a terminal, so captured output is
/// already plain, but stripping keeps parsing correct if that ever changes.
fn strip_ansi(line: &str) -> String {
    let mut stripped = String::with_capacity(line.len());
    let mut chars = line.chars();

    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            stripped.push(c);
            continue;
        }

        // A control sequence ends at its first byte in the range `@`..=`~`,
        // skipping the `[` that introduces it.
        for c in chars.by_ref() {
            if c != '[' && ('@'..='~').contains(&c) {
                break;
            }
        }
    }

    stripped
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim excerpt of `moss list sync` on an `AerynOS` 2026.08 system.
    const MOSS_LIST_SYNC: &str = "\
accountsservice                        26.13.3-10 => 26.27.3-13 - D-BUS encapsulation for user and account management
adwaita-fonts                                  50.0-4 => 51.0-5 - The typefaces for GNOME
boulder        0.26.6+git.42d4675-155 => 0.26.8+git.4e60c77-164 - Modern package building tool
device-mapper-libs                     2.03.41-20 => 2.03.42-22 - Device mapper utility - library
gstreamer-plugins-bad-libs               1.28.4-53 => 1.28.6-58 - GStreamer streaming media framework \"bad\" plugins - libs
libass                                     0.17.4-4 => 0.17.5-5 - libass is a portable subtitle renderer for the ASS/SSA (Advanced Substation Alpha/Substation Alpha) subtitle format.";

    fn moss_updates(output: &str) -> Vec<PackageUpdate> {
        let checker = UpdateChecker::new(PackageManager::Moss);
        output
            .lines()
            .filter_map(|line| checker.parse_package_line(line, false))
            .collect()
    }

    #[test]
    fn parses_every_line_of_real_moss_sync_output() {
        let updates = moss_updates(MOSS_LIST_SYNC);
        assert_eq!(updates.len(), MOSS_LIST_SYNC.lines().count());

        assert_eq!(updates[0].name, "accountsservice");
        assert_eq!(updates[0].current_version, "26.13.3-10");
        assert_eq!(updates[0].new_version, "26.27.3-13");
        assert!(!updates[0].is_aur);
    }

    #[test]
    fn parses_moss_versions_containing_git_revisions() {
        let updates = moss_updates(MOSS_LIST_SYNC);
        let boulder = updates.iter().find(|p| p.name == "boulder").unwrap();

        assert_eq!(boulder.current_version, "0.26.6+git.42d4675-155");
        assert_eq!(boulder.new_version, "0.26.8+git.4e60c77-164");
    }

    #[test]
    fn keeps_summary_text_out_of_the_moss_version() {
        // These summaries contain " - " and " => "-adjacent punctuation that a
        // naive split on the summary separator would trip over.
        let updates = moss_updates(MOSS_LIST_SYNC);

        let dm = updates.iter().find(|p| p.name == "device-mapper-libs").unwrap();
        assert_eq!(dm.new_version, "2.03.42-22");

        let gst = updates
            .iter()
            .find(|p| p.name == "gstreamer-plugins-bad-libs")
            .unwrap();
        assert_eq!(gst.new_version, "1.28.6-58");
    }

    #[test]
    fn ignores_moss_lines_without_a_sync_candidate() {
        // `moss list installed` formatting: no " => ", so nothing to update.
        let updates = moss_updates("acl                                      2.4.0-6 - Access control list utilities");
        assert!(updates.is_empty());
    }

    #[test]
    fn parses_moss_output_that_carries_colour() {
        let coloured = "\u{1b}[1macl\u{1b}[0m   \u{1b}[35m2.3.2\u{1b}[0m-\u{1b}[2m5\u{1b}[0m => \u{1b}[32m2.4.0\u{1b}[0m-\u{1b}[2m6\u{1b}[0m - Access control list utilities";
        let updates = moss_updates(coloured);

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "acl");
        assert_eq!(updates[0].current_version, "2.3.2-5");
        assert_eq!(updates[0].new_version, "2.4.0-6");
    }

    #[test]
    fn strip_ansi_leaves_plain_text_untouched() {
        let plain = "acl 2.3.2-5 => 2.4.0-6 - Access control list utilities";
        assert_eq!(strip_ansi(plain), plain);
    }

    #[test]
    fn moss_is_wired_up_as_a_package_manager() {
        assert_eq!(PackageManager::Moss.name(), "moss");
        // `-u` refreshes the repositories, matching what the applet reported.
        assert_eq!(PackageManager::Moss.system_update_command(), "sudo moss sync -u");
        assert!(!PackageManager::Moss.supports_aur());
    }
}
