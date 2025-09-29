use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
use tokio::process::Command as TokioCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageManager {
    Pacman,
    Paru,
    Yay,
}

impl PackageManager {
    pub fn name(&self) -> &'static str {
        match self {
            PackageManager::Pacman => "pacman",
            PackageManager::Paru => "paru",
            PackageManager::Yay => "yay",
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

        for pm in [PackageManager::Paru, PackageManager::Yay, PackageManager::Pacman] {
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

    pub async fn check_updates(&self, _include_aur: bool) -> Result<UpdateInfo> {
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
                // Continue with AUR check even if official fails
            }
        }

        // Step 2: Only after official check is done, check AUR updates
        if self.package_manager.supports_aur() {
            match self.check_aur_updates().await {
                Ok(aur_updates) => {
                    let count = aur_updates.len();
                    update_info.aur_updates = count;
                    update_info.packages.extend(aur_updates);
                }
                Err(e) => {
                    eprintln!("Failed to check AUR updates: {}", e);
                    // Continue even if AUR check fails
                }
            }
        }

        // Step 3: Calculate final total only after both checks are complete
        update_info.total_updates = update_info.packages.len();

        Ok(update_info)
    }

    async fn check_official_updates(&self) -> Result<Vec<PackageUpdate>> {
        // Always use checkupdates for official packages as it's more reliable
        // and doesn't require database synchronization
        self.parse_update_output("checkupdates", vec![], false).await
    }

    async fn check_aur_updates(&self) -> Result<Vec<PackageUpdate>> {
        let (cmd, args) = match self.package_manager {
            PackageManager::Pacman => return Ok(Vec::new()),
            PackageManager::Paru => ("paru", vec!["-Qu", "--aur"]),
            PackageManager::Yay => ("yay", vec!["-Qu", "--aur"]),
        };

        self.parse_update_output(cmd, args, true).await
    }

    async fn parse_update_output(&self, cmd: &str, args: Vec<&str>, is_aur: bool) -> Result<Vec<PackageUpdate>> {
        let output = TokioCommand::new(cmd)
            .args(&args)
            .output()
            .await?;

        if !output.status.success() {
            let exit_code = output.status.code().unwrap_or(-1);
            if exit_code == 1 || exit_code == 2 {
                // No updates available
                // checkupdates typically returns 2, paru/yay typically return 1
                eprintln!("No updates available (exit code: {})", exit_code);
                return Ok(Vec::new());
            }
            eprintln!("Update check failed with exit code: {}", exit_code);
            return Err(anyhow!("Failed to check for updates: {}", String::from_utf8_lossy(&output.stderr)));
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
        // Handle different output formats
        if line.contains(" -> ") {
            // Format: "package 1.0.0-1 -> 1.0.1-1"
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
            // Format: "package 1.0.1-1"
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

        None
    }

}