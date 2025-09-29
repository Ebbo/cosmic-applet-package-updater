# COSMIC Package Updater Applet

A modern package update manager applet for the COSMIC desktop environment on Arch Linux, providing seamless package management directly from the panel.

## Features

### 📦 **Package Manager Support**
- **Pacman**: Official Arch Linux repository packages
- **Paru**: Recommended - supports both official and AUR packages
- **Yay**: Alternative AUR helper with official package support
- **Auto-detection**: Automatically discovers available package managers

### 🔄 **Update Management**
- **Visual Indicators**: Panel icon changes based on update status
  - Green security shield: System up to date
  - Orange update arrow: Updates available with count badge
  - Refresh spinner: Checking for updates
  - Red error symbol: Error occurred
- **Update Checking**: Manual and automatic update checking
- **Database Updates**: Refresh package database (pacman -Sy equivalent)
- **System Updates**: Launch terminal with update commands
- **Detailed Breakdown**: Separate counts for official vs AUR packages

### 🎨 **User Interface**
- **Two-Tab Popup Window**:
  - **Updates Tab**: Current status, update counts, action buttons
  - **Settings Tab**: Configuration options and preferences
- **Panel Integration**: Compact icon with visual status indicators
- **Real-time Status**: Shows current update state and counts

### ⚙️ **Configuration Options**
- **Package Manager Selection**: Choose between Pacman/Paru/Yay
- **Check Interval**: Configurable from 5 minutes to 24 hours
- **Auto-check on Startup**: Automatically check for updates when applet starts
- **Include AUR Updates**: Toggle AUR package inclusion
- **Show Notifications**: Desktop notifications for available updates
- **Show Update Count**: Toggle update count badge in panel
- **Preferred Terminal**: Custom terminal command for running updates

### ⌨️ **Mouse Interactions**
- **Left Click**: Toggle popup window
- **Middle Click**: Quick update check
- **Scroll Up**: Check for updates
- **Scroll Down**: Refresh package database

### 🔧 **Background Operations**
- **Periodic Checking**: Automatic update checks based on configured interval
- **Non-blocking Operations**: All package manager calls run asynchronously
- **Resource Efficient**: Minimal system impact when idle

## Installation

### Arch Linux (AUR)

The applet will be available as an **AUR package**:

```bash
paru -S cosmic-applet-package-updater-git
```

or with yay:

```bash
yay -S cosmic-applet-package-updater-git
```

### Build from Source

1. **Clone the repository**:
   ```bash
   git clone https://github.com/Ebbo/cosmic-applet-package-updater.git
   cd cosmic-applet-package-updater
   ```

2. **Install Just build tool** (if not already installed):
   ```bash
   cargo install just
   ```

3. **Build the applet**:
   ```bash
   just build-release
   ```

4. **Install system-wide**:
   ```bash
   sudo just install
   ```

### Prerequisites

- Rust 1.80+
- COSMIC desktop environment
- Just build tool (`cargo install just`)
- Git (for cloning)
- At least one supported package manager (pacman/paru/yay)

## Development

For development and testing:

```bash
# Build debug version
just build-debug

# Run with debug logging
just run

# Format code and run
just dev

# Run clippy linting
just check

# Clean build artifacts
just clean
```

## Usage

### Adding the Applet to COSMIC Panel

After installation, add the Package Updater applet to your COSMIC panel:

1. **Open COSMIC Settings**
2. **Navigate to Desktop → Panel → Configure panel applets**
3. **Find "Package Updater" in the available applets list**
4. **Click to add it to your panel**

The applet will appear as a status icon in your COSMIC panel.

### Using the Applet

1. **Basic Control**: Click the package icon to open the control popup
2. **Updates Tab**:
   - View current update status and counts
   - See breakdown of official vs AUR updates
   - Use "Check Again" and "Update Database" buttons
   - Launch system update with "Update System" button
3. **Settings Tab**:
   - Select package manager (Pacman/Paru/Yay)
   - Configure check interval and auto-check options
   - Toggle AUR updates, notifications, and count display
   - Set preferred terminal for updates
4. **Quick Actions**:
   - Middle-click for quick update check
   - Scroll for update checking and database refresh

## Configuration

### Package Manager Selection

The applet provides flexible package manager management:

1. **Auto-Discovery**: Automatically detects available package managers
2. **Manual Selection**: Choose your preferred package manager
3. **AUR Support**: Enable/disable AUR packages based on selected manager

### Update Checking

- **Manual**: Click "Check Again" or use mouse shortcuts
- **Automatic**: Configure interval-based checking (5 min - 24 hours)
- **On Startup**: Optionally check for updates when applet starts

### Configuration Files

Settings are stored in:
- `~/.config/cosmic/com.cosmic.PackageUpdater/`

## Supported Package Managers

- **Pacman**: Official Arch repository packages only
- **Paru**: Official and AUR packages (recommended)
- **Yay**: Official and AUR packages (alternative)

## Technical Details

- **Framework**: Built with libcosmic (COSMIC's UI toolkit)
- **Language**: Rust
- **Async Operations**: Non-blocking package manager interactions
- **Regex Parsing**: Robust parsing of package manager output
- **Configuration**: Persistent settings with cosmic-config

## Security & Safety

- **Read-only Operations**: Update checking never modifies system
- **Terminal Execution**: System updates run in user's preferred terminal
- **Permission Handling**: Proper sudo prompting through terminal
- **Input Validation**: Sanitized package manager command execution

## License

This project is licensed under the GPL-3.0 License - see the LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit pull requests or open issues for bugs and feature requests.

## Troubleshooting

### Applet not appearing in panel
- Ensure the applet is properly installed: `which package-updater` should return a path
- Try restarting COSMIC or logging out/in
- Check COSMIC Settings → Desktop → Panel → Configure panel applets

### No package managers found
- Install a supported package manager (paru recommended)
- Click "Discover Package Managers" in Settings tab
- Ensure the package manager is in your PATH

### Updates not showing
- Check that the correct package manager is selected
- Try "Check Again" or "Update Database"
- Verify your package manager works from command line

### Terminal not launching
- Check the preferred terminal setting in Settings tab
- Ensure the specified terminal is installed and in PATH
- Default is "cosmic-term" - change if using different terminal

### Permission errors
- Package database updates may require sudo (pacman only)
- System updates will prompt for password in terminal
- Ensure your user has appropriate sudo privileges

## Building Requirements

The following system packages are required for building:

- `pkg-config` (for dependency detection)
- `build-essential` or equivalent (C compiler for native dependencies)

### Ubuntu/Debian:
```bash
sudo apt install pkg-config build-essential
```

### Fedora/RHEL:
```bash
sudo dnf install pkgconfig gcc
```

### Arch Linux:
```bash
sudo pacman -S pkg-config base-devel
```

### openSUSE:
```bash
sudo zypper install pkg-config gcc
```