use crate::backend::events::SpvEvent;
use crate::backend::types::{SyncProgress, SyncState};

/// Connection and sync state machine.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Syncing(SyncProgressView),
    Synced,
    Error(String),
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::Disconnected
    }
}

impl ConnectionState {
    /// Apply an SPV event and transition to the next state.
    pub fn apply_event(&mut self, event: &SpvEvent) {
        match event {
            SpvEvent::PeerConnected(_) => {
                if matches!(self, Self::Disconnected) {
                    *self = Self::Connecting;
                }
            }
            SpvEvent::PeerDisconnected(_) => {}
            SpvEvent::PeersUpdated { count, .. } => {
                if *count == 0 && !matches!(self, Self::Error(_)) {
                    *self = Self::Disconnected;
                }
            }
            SpvEvent::SyncProgressUpdated(progress) => {
                if !matches!(self, Self::Error(_)) {
                    if progress.is_synced() {
                        *self = Self::Synced;
                    } else {
                        *self = Self::Syncing(SyncProgressView::from_progress(progress));
                    }
                }
            }
            SpvEvent::SyncComplete { .. } => {
                if !matches!(self, Self::Error(_)) {
                    *self = Self::Synced;
                }
            }
            SpvEvent::Error(msg) => {
                *self = Self::Error(msg.clone());
            }
            _ => {}
        }
    }

}

/// Display-ready sync progress.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncProgressView {
    pub percentage: f64,
    pub stage_label: String,
}

impl SyncProgressView {
    pub fn from_progress(progress: &SyncProgress) -> Self {
        let stage_label = format_sync_stage(progress);
        Self {
            percentage: progress.percentage() * 100.0,
            stage_label,
        }
    }
}

fn format_sync_stage(progress: &SyncProgress) -> String {
    if let Ok(p) = progress.headers()
        && p.state() == SyncState::Syncing
    {
        return "Syncing headers...".to_string();
    }
    if let Ok(p) = progress.filter_headers()
        && p.state() == SyncState::Syncing
    {
        return "Syncing filter headers...".to_string();
    }
    if let Ok(p) = progress.filters()
        && p.state() == SyncState::Syncing
    {
        return "Downloading filters...".to_string();
    }
    if let Ok(p) = progress.blocks()
        && p.state() == SyncState::Syncing
    {
        return "Processing blocks...".to_string();
    }
    if let Ok(p) = progress.masternodes()
        && p.state() == SyncState::Syncing
    {
        return "Syncing masternodes...".to_string();
    }
    if let Ok(p) = progress.chainlocks()
        && p.state() == SyncState::Syncing
    {
        return "Validating chain locks...".to_string();
    }
    if let Ok(p) = progress.instantsend()
        && p.state() == SyncState::Syncing
    {
        return "Processing instant send...".to_string();
    }

    if progress.is_synced() {
        "Synced".to_string()
    } else {
        let pct = progress.percentage() * 100.0;
        format!("Syncing... ({pct:.0}%)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnected_to_connecting_on_peer_connected() {
        let mut state = ConnectionState::Disconnected;
        state.apply_event(&SpvEvent::PeerConnected("1.2.3.4:9999".into()));
        assert!(matches!(state, ConnectionState::Connecting));
    }

    #[test]
    fn connecting_to_syncing_on_progress() {
        let mut state = ConnectionState::Connecting;
        let progress = SyncProgress::default();
        state.apply_event(&SpvEvent::SyncProgressUpdated(Box::new(progress)));
        assert!(matches!(state, ConnectionState::Syncing(_)));
    }

    #[test]
    fn syncing_to_synced_on_complete() {
        let mut state = ConnectionState::Syncing(SyncProgressView {
            percentage: 99.0,
            stage_label: "Syncing...".into(),
        });
        state.apply_event(&SpvEvent::SyncComplete {
            tip_height: 1000,
            cycle: 1,
        });
        assert_eq!(state, ConnectionState::Synced);
    }

    #[test]
    fn error_from_any_state() {
        for initial in [
            ConnectionState::Disconnected,
            ConnectionState::Connecting,
            ConnectionState::Synced,
        ] {
            let mut state = initial;
            state.apply_event(&SpvEvent::Error("connection lost".into()));
            assert!(matches!(state, ConnectionState::Error(_)));
        }
    }

    #[test]
    fn events_ignored_during_error() {
        let mut state = ConnectionState::Error("test".into());
        state.apply_event(&SpvEvent::SyncComplete {
            tip_height: 1000,
            cycle: 1,
        });
        assert!(matches!(state, ConnectionState::Error(_)));
    }

    #[test]
    fn disconnected_on_zero_peers() {
        let mut state = ConnectionState::Connecting;
        state.apply_event(&SpvEvent::PeersUpdated {
            count: 0,
            best_height: 0,
        });
        assert_eq!(state, ConnectionState::Disconnected);
    }

    #[test]
    fn default_progress_shows_syncing_fallback() {
        let view = SyncProgressView::from_progress(&SyncProgress::default());
        assert_eq!(view.percentage, 0.0);
        assert_eq!(view.stage_label, "Syncing... (0%)");
    }
}
