use anyhow::Result;
use zbus::Connection;

/// DBus interface for notifying other applets about package updates
pub struct UpdateNotifier {
    connection: Connection,
}

impl UpdateNotifier {
    /// Create a new UpdateNotifier
    pub async fn new() -> Result<Self> {
        let connection = Connection::session().await?;
        Ok(Self { connection })
    }

    /// Notify other applets that package updates have been completed
    /// This sends a DBus signal that other package-related applets can listen to
    pub async fn notify_update_completed(&self) -> Result<()> {
        // Send a custom DBus signal for package update completion
        self.connection.emit_signal(
            None::<&str>,
            "/com/cosmic/PackageUpdater",
            "com.cosmic.PackageUpdater",
            "UpdateCompleted",
            &(),
        ).await?;

        Ok(())
    }

    /// Broadcast a generic signal that other applets can listen to
    /// This signal indicates that system packages have been updated
    pub async fn broadcast_system_updated(&self) -> Result<()> {
        // Create a more generic signal that any system monitoring applet could use
        self.connection.emit_signal(
            None::<&str>,
            "/org/freedesktop/PackageKit",
            "org.freedesktop.PackageKit",
            "UpdatesChanged",
            &(),
        ).await?;

        Ok(())
    }
}