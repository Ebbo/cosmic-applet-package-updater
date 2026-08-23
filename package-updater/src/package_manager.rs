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
            // AerynOS: moss reports a whole sync plan rather than one package
            // per line, so it needs its own command and parser.
            PackageManager::Moss => return self.check_moss_updates().await,
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
    /// Ask moss what `moss sync` would actually do.
    ///
    /// `moss list sync` is not enough: it only reports a package whose source
    /// release differs from the installed one, so it misses packages rebuilt at
    /// the same release and dependencies the sync would newly pull in. `moss
    /// sync` applies both, so the applet would keep reporting fewer updates than
    /// the update button goes on to install.
    async fn check_moss_updates(&self) -> Result<Vec<PackageUpdate>> {
        // moss can only offer candidates that are already in its repository
        // index. Refreshing the system index needs privileges, so refresh a
        // private one and fall back to the system index when that fails.
        let root = Self::refreshed_moss_root().await;
        let mut args: Vec<&str> = Vec::new();
        if let Some(root) = &root {
            args.extend(["-D", root.as_str()]);
        }
        // A dry run resolves the sync without downloading or writing anything.
        args.extend(["sync", "--dry-run"]);

        let output = TokioCommand::new("moss").args(&args).output().await?;

        if !output.status.success() {
            return Err(anyhow!(
                "moss sync --dry-run failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        Ok(parse_moss_sync_plan(&String::from_utf8_lossy(&output.stdout)))
    }

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

            // moss prints a whole sync plan rather than one package per line,
            // so it is parsed by `parse_moss_sync_plan` instead.
            PackageManager::Moss => {}

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

/// Parse the plan that `moss sync --dry-run` prints.
///
/// The plan is laid out in columns that reflow with the terminal width, so it is
/// parsed as a stream of whitespace separated tokens instead of column by column.
/// Neither package names nor versions contain whitespace, which makes the ` -> `
/// of an update a reliable anchor:
///
/// ```text
/// The following packages will be added:
///
/// passim  0.1.12-1
///
/// The following packages will be updated:
///
/// fwupd  2.1.7-1 -> 2.1.7-1   modemmanager  1.24.2-6 -> 1.24.2-1
/// ```
fn parse_moss_sync_plan(output: &str) -> Vec<PackageUpdate> {
    enum Section {
        Added,
        Updated,
        Ignored,
    }

    let mut section = Section::Ignored;
    let mut added: Vec<String> = Vec::new();
    let mut updated: Vec<String> = Vec::new();

    for line in output.lines() {
        let line = strip_ansi(line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with("The following") {
            // Orphaned removals also land here and are deliberately ignored:
            // they are a consequence of the other changes, not an update.
            section = if line.contains(" will be added") {
                Section::Added
            } else if line.contains(" will be updated") {
                Section::Updated
            } else {
                Section::Ignored
            };
            continue;
        }

        match section {
            Section::Added => added.extend(line.split_whitespace().map(str::to_string)),
            Section::Updated => updated.extend(line.split_whitespace().map(str::to_string)),
            Section::Ignored => {}
        }
    }

    // Every update reads `name old -> new`, so anchor each one on its arrow.
    let mut packages: Vec<PackageUpdate> = updated
        .iter()
        .enumerate()
        .filter(|(i, token)| *token == "->" && *i >= 2 && i + 1 < updated.len())
        .map(|(i, _)| PackageUpdate {
            name: updated[i - 2].clone(),
            current_version: updated[i - 1].clone(),
            new_version: updated[i + 1].clone(),
            is_aur: false,
        })
        .collect();

    // Additions read `name version`. They carry no installed version, which the
    // applet renders by showing the name on its own.
    packages.extend(added.chunks_exact(2).map(|entry| PackageUpdate {
        name: entry[0].clone(),
        current_version: "unknown".to_string(),
        new_version: entry[1].clone(),
        is_aur: false,
    }));

    packages
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

    /// Verbatim `moss sync --dry-run` output on an `AerynOS` 2026.08 system,
    /// trailing spaces and all.
    const MOSS_SYNC_PLAN: &str = "\
The following packages will be added: \n\
\n\
passim  0.1.12-1   \n\
\n\
The following packages will be updated: \n\
\n\
fwupd           2.1.7-1 -> 2.1.7-1   modemmanager  1.24.2-6 -> 1.24.2-1   python-scipy  1.18.0-1 -> 1.18.0-1   reaction        2.5.1-2 -> 2.5.1-3   \n\
\n\
The following orphaned packages will be removed: \n\
\n\
modemmanager-glib  1.24.2-6   \n";

    fn find<'a>(updates: &'a [PackageUpdate], name: &str) -> &'a PackageUpdate {
        updates
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("{name} missing from {updates:?}"))
    }

    #[test]
    fn parses_every_package_in_a_moss_sync_plan() {
        let updates = parse_moss_sync_plan(MOSS_SYNC_PLAN);

        // Four updates plus the one addition; the orphaned removal is not one.
        assert_eq!(updates.len(), 5);

        let modemmanager = find(&updates, "modemmanager");
        assert_eq!(modemmanager.current_version, "1.24.2-6");
        assert_eq!(modemmanager.new_version, "1.24.2-1");
        assert!(!modemmanager.is_aur);

        let reaction = find(&updates, "reaction");
        assert_eq!(reaction.current_version, "2.5.1-2");
        assert_eq!(reaction.new_version, "2.5.1-3");
    }

    #[test]
    fn reports_moss_packages_rebuilt_at_the_same_release() {
        // The reason this reads the sync plan instead of `moss list sync`: both
        // of these are rebuilds at an unchanged source release, so `list sync`
        // hides them even though `moss sync` replaces them.
        let updates = parse_moss_sync_plan(MOSS_SYNC_PLAN);

        let fwupd = find(&updates, "fwupd");
        assert_eq!(fwupd.current_version, "2.1.7-1");
        assert_eq!(fwupd.new_version, "2.1.7-1");

        assert_eq!(find(&updates, "python-scipy").new_version, "1.18.0-1");
    }

    #[test]
    fn reports_packages_a_moss_sync_would_newly_pull_in() {
        // `moss list sync` walks installed packages only, so it never mentions a
        // dependency the sync is about to add.
        let passim = find(&parse_moss_sync_plan(MOSS_SYNC_PLAN), "passim").clone();

        assert_eq!(passim.new_version, "0.1.12-1");
        // Nothing is installed yet, which the applet renders as a bare name.
        assert_eq!(passim.current_version, "unknown");
    }

    #[test]
    fn leaves_moss_orphan_removals_out_of_the_update_list() {
        let updates = parse_moss_sync_plan(MOSS_SYNC_PLAN);
        assert!(!updates.iter().any(|p| p.name == "modemmanager-glib"));
    }

    #[test]
    fn parses_a_moss_plan_that_reflowed_to_one_entry_per_line() {
        // moss packs entries into as many columns as the terminal is wide, so
        // the same plan has to parse identically however it was wrapped.
        let narrow = "\
The following packages will be added: \n\
\n\
passim  0.1.12-1   \n\
\n\
The following packages will be updated: \n\
\n\
fwupd           2.1.7-1 -> 2.1.7-1   \n\
modemmanager  1.24.2-6 -> 1.24.2-1   \n\
python-scipy  1.18.0-1 -> 1.18.0-1   \n\
reaction        2.5.1-2 -> 2.5.1-3   \n\
\n\
The following orphaned packages will be removed: \n\
\n\
modemmanager-glib  1.24.2-6   \n";

        let wide = parse_moss_sync_plan(MOSS_SYNC_PLAN);
        let narrow = parse_moss_sync_plan(narrow);

        assert_eq!(narrow.len(), wide.len());
        for (narrow, wide) in narrow.iter().zip(&wide) {
            assert_eq!(narrow.name, wide.name);
            assert_eq!(narrow.current_version, wide.current_version);
            assert_eq!(narrow.new_version, wide.new_version);
        }
    }

    #[test]
    fn parses_moss_versions_containing_git_revisions() {
        // Reformatted from a real plan: the version carries a git revision, so
        // splitting a version on `-` or `.` would lose part of it.
        let plan = "\
The following packages will be updated: \n\
\n\
boulder  0.26.6+git.42d4675-155 -> 0.26.8+git.4e60c77-164   \n";

        let updates = parse_moss_sync_plan(plan);

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "boulder");
        assert_eq!(updates[0].current_version, "0.26.6+git.42d4675-155");
        assert_eq!(updates[0].new_version, "0.26.8+git.4e60c77-164");
    }

    #[test]
    fn parses_a_moss_plan_that_carries_colour() {
        let coloured = "\
The following packages will be updated: \n\
\n\
\u{1b}[1macl\u{1b}[0m  \u{1b}[35m2.3.2\u{1b}[0m-\u{1b}[2m5\u{1b}[0m -> \u{1b}[32m2.4.0\u{1b}[0m-\u{1b}[2m6\u{1b}[0m   \n";

        let updates = parse_moss_sync_plan(coloured);

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "acl");
        assert_eq!(updates[0].current_version, "2.3.2-5");
        assert_eq!(updates[0].new_version, "2.4.0-6");
    }

    #[test]
    fn reports_nothing_when_a_moss_sync_has_no_work() {
        assert!(parse_moss_sync_plan("").is_empty());
        assert!(parse_moss_sync_plan("No changes to make\n").is_empty());
    }

    #[test]
    fn strip_ansi_leaves_plain_text_untouched() {
        let plain = "fwupd  2.1.7-1 -> 2.1.7-1   modemmanager  1.24.2-6 -> 1.24.2-1";
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
