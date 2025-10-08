# Package Updater Applet for COSMIC™

A lightweight and efficient package update notifier applet for the COSMIC™ desktop. Stay informed about system updates with real-time notifications and seamless integration into your COSMIC panel.

Supports multiple Linux distributions including Arch Linux, Debian/Ubuntu, Fedora, openSUSE, Alpine, and more!

![Main Interface](screenshots/Package-Updater-Main.png)

## Features

### 📦 **Package Manager Support**
- **Arch Linux**: Pacman, Paru, Yay (with AUR support)
- **Debian/Ubuntu/Pop!_OS**: APT
- **Fedora/RHEL**: DNF
- **openSUSE/SUSE**: Zypper
- **Alpine Linux**: APK
- **Universal**: Flatpak
- **Auto-detection**: Automatically discovers available package managers on first launch

### 🔄 **Update Management**
- **Visual Indicators**: Panel icon changes based on update status
  - ✅ Package icon: System up to date
  - 🎁 Update icon with count: Updates available
  - ⏳ Refresh spinner: Checking for updates
  - ❌ Error icon: Error occurred
- **Automatic Checking**: Configurable interval-based update checking (default: 60 minutes)
- **One-Click Updates**: Launch system updates directly from the applet in your preferred terminal
- **Detailed Package List**: View all available updates with version information (AUR packages shown separately on Arch-based systems)
- **Instance Synchronization**: Multiple applet instances stay in sync automatically

### 🎨 **User Interface**
- **Clean Two-Tab Layout**:
  - **Updates Tab**: Shows update status, package list, and action buttons
  - **Settings Tab**: Configure all preferences in one place
- **Visual Package Illustration**: Dynamic icon and emoji showing current status
- **Smart Button Placement**: Update System button appears only when updates are available
- **Scrollable Package List**: View all updates in an organized, easy-to-read format

### ⚙️ **Configuration Options**
- **Package Manager Selection**: Choose from detected package managers
- **Check Interval**: Set how often to check for updates (1-1440 minutes)
- **Auto-check on Startup**: Automatically check for updates when applet starts
- **Include AUR Updates**: Toggle AUR package update detection (Arch Linux only)
- **Show Notifications**: Enable/disable update notifications (feature ready)
- **Show Update Count**: Display the number of updates in the panel icon
- **Preferred Terminal**: Set your preferred terminal emulator (default: cosmic-term)

### ⌨️ **Quick Actions**
- **Left Click**: Open the applet popup window
- **Middle Click on Panel Icon**: Launch system update directly
- **Update System Button**: Opens terminal with update command, then automatically re-checks

### 🔧 **Smart Background Operations**
- **File-Based Locking**: Prevents multiple instances from checking simultaneously
- **Automatic Retry Logic**: Retries failed checks once to handle temporary errors
- **File Watcher Sync**: When one instance checks for updates, all others sync within 100ms
- **Post-Update Check**: Automatically re-checks for updates after terminal closes (3-second stabilization delay)
- **Resource Efficient**: Minimal system impact when idle

## Screenshots

### Updates Tab
![Updates Available](screenshots/Package-Updater-Updates.png)
*The main updates tab showing available updates with package details*

### Settings Tab
![Settings](screenshots/Package-Updater-Settings.png)
*Configure package manager, intervals, and preferences*

## Installation

### From AUR (Recommended)

Install using your preferred AUR helper:

```bash
# Using paru
paru -S cosmic-applet-package-updater-git

# Using yay
yay -S cosmic-applet-package-updater-git
```

**AUR Package**: [cosmic-applet-package-updater-git](https://aur.archlinux.org/packages/cosmic-applet-package-updater-git)

### Build from Source

1. **Clone the repository**:
   ```bash
   git clone https://github.com/Ebbo/cosmic-applet-package-updater.git
   cd cosmic-applet-package-updater
   ```

2. **Build and install** (using just):
   ```bash
   just build-release
   sudo just install
   ```

   Or manually with cargo:
   ```bash
   cd package-updater
   cargo build --release
   sudo install -Dm755 target/release/cosmic-ext-applet-package-updater /usr/bin/cosmic-ext-applet-package-updater
   ```

### Prerequisites

#### All Distributions
- **Desktop Environment**: COSMIC™ desktop
- **Rust**: 1.80 or newer (for building from source)
- **Terminal Emulator**: cosmic-term (recommended) or any terminal supporting `-e` flag

#### Build Dependencies

**Arch Linux / Manjaro:**
```bash
sudo pacman -S rust cargo base-devel
```

**Debian / Ubuntu / Pop!_OS:**
```bash
sudo apt install cargo libxkbcommon-dev pkg-config
```

**Fedora / RHEL:**
```bash
sudo dnf install cargo libxkbcommon-devel pkgconfig
```

**openSUSE:**
```bash
sudo zypper install cargo libxkbcommon-devel pkg-config
```

#### Runtime Dependencies (Distribution-Specific)

**Arch Linux:**
- `pacman-contrib` (for `checkupdates` command)
- Optional: `paru` or `yay` for AUR support

**Debian/Ubuntu/Pop!_OS:**
- `apt` (pre-installed)

**Fedora/RHEL:**
- `dnf` (pre-installed)

**openSUSE:**
- `zypper` (pre-installed)

**Alpine:**
- `apk` (pre-installed)

**Universal (any distribution):**
- `flatpak` (optional)

## Usage

### Adding the Applet to COSMIC Panel

After installation, add the Package Updater applet to your COSMIC panel:

1. Right-click on the COSMIC panel
2. Select "Panel Settings" or "Configure Panel"
3. Find "Package Updater" in the available applets list
4. Click to add it to your panel

The applet will appear as an icon in your COSMIC panel.

### Using the Applet

**Updates Tab**:
- View current update status with visual indicators
- See detailed package list with version information
- Packages are organized into Official and AUR categories
- Click "Check for Updates" to manually refresh
- Click "Update System" to launch updates in terminal (appears only when updates available)
- Tip displayed: "Middle-click on the Panel icon" for quick updates

**Settings Tab**:
- **Package Manager**: Select from detected package managers
- **Check Interval**: Set minutes between automatic checks (1-1440)
- **Auto-check on startup**: Toggle automatic checking when applet starts
- **Include AUR updates**: Enable/disable AUR package detection (only shown on Arch Linux with Paru/Yay)
- **Show notifications**: Enable/disable update notifications
- **Show update count**: Toggle update count badge on panel icon
- **Preferred Terminal**: Set terminal command (default: cosmic-term)

**Quick Actions**:
- **Left Click**: Open/close applet popup
- **Middle Click on Panel Icon**: Launch system update immediately

### How Updates Work

1. **Checking for Updates**:
   - Automatic checks run based on your configured interval
   - Manual checks via "Check for Updates" button
   - File-based locking prevents simultaneous checks across instances

2. **Installing Updates**:
   - Click "Update System" or middle-click the panel icon
   - Terminal opens with update command for your package manager
   - Complete the update process in the terminal
   - Close terminal when done
   - Applet automatically re-checks for updates after 3 seconds
   - All applet instances sync the new state within 100ms

3. **Instance Synchronization**:
   - Multiple applet instances stay synchronized automatically
   - When one instance checks for updates, others sync via file watcher
   - Prevents duplicate checks with file-based locking

## Configuration

Settings are stored in:
```
~/.config/cosmic/com.github.cosmic_ext.PackageUpdater/
```

Lock and sync files (automatically managed):
```
$XDG_RUNTIME_DIR/cosmic-package-updater.lock
$XDG_RUNTIME_DIR/cosmic-package-updater.sync
```

## How It Works

### Update Detection

The applet uses distribution-specific commands to detect updates:

**Arch Linux:**
- **Official Packages**: `checkupdates` (from pacman-contrib)
- **AUR Packages (Paru)**: `paru -Qu --aur`
- **AUR Packages (Yay)**: `yay -Qu --aur`

**Debian/Ubuntu/Pop!_OS:**
- `apt list --upgradable`

**Fedora/RHEL:**
- `dnf check-update -q`

**openSUSE/SUSE:**
- `zypper list-updates`

**Alpine:**
- `apk -u list`

**Flatpak:**
- `flatpak remote-ls --updates`

### Smart Features

- **Retry Logic**: Failed checks are automatically retried once after 1 second
- **Exit Code Handling**: Correctly interprets exit codes (2 for checkupdates, 1 for paru/yay means no updates)
- **File-Based Locking**: Uses `$XDG_RUNTIME_DIR/cosmic-package-updater.lock` to prevent simultaneous checks
- **File Watcher Sync**: Monitors `$XDG_RUNTIME_DIR/cosmic-package-updater.sync` to sync instances
- **Debouncing**: 3-second minimum between syncs to prevent rapid repeated checks

## Technical Details

- **Framework**: Built with libcosmic
- **Language**: Rust
- **Async Operations**: All package manager calls are non-blocking (tokio)
- **Configuration**: Persistent settings with cosmic-config
- **File Watching**: Uses the `notify` crate for instance synchronization

## Troubleshooting

### Applet not appearing in panel
- Ensure the applet is properly installed: `which cosmic-ext-applet-package-updater` should return a path
- Restart the COSMIC panel or log out and back in
- Check COSMIC Settings → Desktop → Panel settings

### No package managers found
- **Arch Linux**: Install `pacman-contrib` for the `checkupdates` command: `sudo pacman -S pacman-contrib`
- **Arch Linux (AUR)**: Install `paru` or `yay` for AUR support
- **Other distros**: The default package manager (apt/dnf/zypper/apk) should be pre-installed
- Click "Discover Package Managers" button in the Settings tab
- Ensure package managers are in your `$PATH`

### Updates not showing correctly
- **Arch Linux**: Verify `checkupdates` works from command line: `checkupdates`
- **Debian/Ubuntu**: Try `apt list --upgradable` from command line
- **Fedora**: Try `dnf check-update` from command line
- Check that the correct package manager is selected in Settings
- Try clicking "Check for Updates" manually
- Check system logs for error messages

### Applet keeps checking repeatedly on startup
- This was a bug that has been fixed
- The first sync event on startup is now ignored
- Only syncs when last check was more than 3 seconds ago

### Multiple instances out of sync
- The file watcher should automatically sync all instances
- If issues persist, remove sync file: `rm $XDG_RUNTIME_DIR/cosmic-package-updater.sync`
- Restart the applet

### Terminal not launching
- Verify the preferred terminal setting in Settings tab
- Ensure the terminal is installed: `which cosmic-term`
- Try a different terminal like `konsole` or `kitty`

### "Update check already in progress" errors
- Another instance is currently checking for updates
- The lock file prevents simultaneous checks
- Wait a few seconds and try again
- If persistent, remove lock file: `rm $XDG_RUNTIME_DIR/cosmic-package-updater.lock`

## Contributing

Contributions are welcome! Please feel free to submit pull requests or open issues for bugs and feature requests.

## License

This project is licensed under the GPL-3.0 License - see the LICENSE file for details.

## Credits

Developed for the COSMIC™ desktop community.
