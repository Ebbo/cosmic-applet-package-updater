use anyhow::Result;
use zbus::Connection;
use std::collections::HashMap;

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

        // Also send a notification to the freedesktop notification service
        // This ensures applets that listen to standard notifications also get notified
        self.send_desktop_notification().await?;

        Ok(())
    }

    /// Send a standard desktop notification about update completion
    async fn send_desktop_notification(&self) -> Result<()> {
        // Use the org.freedesktop.Notifications interface directly
        let _notification_id: u32 = self.connection.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "COSMIC Package Updater",  // app_name
                0u32,                      // replaces_id
                "package-x-generic",       // app_icon
                "System Updates Completed", // summary
                "Package updates have been installed. Other applets will refresh their status.", // body
                Vec::<String>::new(),      // actions
                HashMap::<String, zbus::zvariant::Value>::new(), // hints
                5000i32,                   // timeout (5 seconds)
            ),
        ).await?.body().deserialize()?;

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