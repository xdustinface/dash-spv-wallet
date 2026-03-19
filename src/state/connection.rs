use crate::backend::events::SpvEvent;
use crate::backend::types::SyncProgress;

/// Connection and sync state machine.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Syncing(SyncProgressView),
    Synced,
    Paused,
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
                if *count == 0 && !matches!(self, Self::Paused | Self::Error(_)) {
                    *self = Self::Disconnected;
                }
            }
            SpvEvent::SyncProgressUpdated(progress) => {
                if !matches!(self, Self::Paused | Self::Error(_)) {
                    if progress.is_synced {
                        *self = Self::Synced;
                    } else {
                        *self = Self::Syncing(SyncProgressView::from_progress(progress));
                    }
                }
            }
            SpvEvent::SyncComplete { .. } => {
                if !matches!(self, Self::Paused | Self::Error(_)) {
                    *self = Self::Synced;
                }
            }
            SpvEvent::Error(msg) => {
                *self = Self::Error(msg.clone());
            }
            _ => {}
        }
    }

    /// Transition to paused state. Only valid from Syncing or Synced.
    pub fn pause(&mut self) {
        if matches!(self, Self::Syncing(_) | Self::Synced | Self::Connecting) {
            *self = Self::Paused;
        }
    }

    /// Transition from paused back to connecting.
    pub fn resume(&mut self) {
        if matches!(self, Self::Paused) {
            *self = Self::Connecting;
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
            percentage: progress.percentage,
            stage_label,
        }
    }
}

fn format_sync_stage(progress: &SyncProgress) -> String {
    use crate::backend::types::{ManagerId, SyncState};

    for mp in &progress.managers {
        if mp.state == SyncState::Syncing {
            let pct = mp.percentage;
            return match mp.manager {
                ManagerId::Headers => format!("Syncing headers... ({pct:.0}%)"),
                ManagerId::FilterHeaders => format!("Syncing filter headers... ({pct:.0}%)"),
                ManagerId::Filters => format!("Downloading filters... ({pct:.0}%)"),
                ManagerId::Blocks => format!("Processing blocks... ({pct:.0}%)"),
                ManagerId::Masternodes => format!("Syncing masternodes... ({pct:.0}%)"),
                ManagerId::ChainLocks => format!("Validating chain locks... ({pct:.0}%)"),
                ManagerId::InstantSend => format!("Processing instant send... ({pct:.0}%)"),
            };
        }
    }

    if progress.is_synced {
        "Synced".to_string()
    } else {
        format!("Syncing... ({:.0}%)", progress.percentage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::types::{ManagerId, ManagerProgress, SyncState};

    fn syncing_progress(pct: f64, manager: ManagerId) -> SyncProgress {
        SyncProgress {
            state: SyncState::Syncing,
            percentage: pct,
            is_synced: false,
            managers: vec![ManagerProgress {
                manager,
                state: SyncState::Syncing,
                current_height: 500,
                target_height: 1000,
                percentage: pct,
            }],
        }
    }

    fn synced_progress() -> SyncProgress {
        SyncProgress {
            state: SyncState::Synced,
            percentage: 100.0,
            is_synced: true,
            managers: vec![],
        }
    }

    #[test]
    fn disconnected_to_connecting_on_peer_connected() {
        let mut state = ConnectionState::Disconnected;
        state.apply_event(&SpvEvent::PeerConnected("1.2.3.4:9999".into()));
        assert!(matches!(state, ConnectionState::Connecting));
    }

    #[test]
    fn connecting_to_syncing_on_progress() {
        let mut state = ConnectionState::Connecting;
        let progress = syncing_progress(25.0, ManagerId::Headers);
        state.apply_event(&SpvEvent::SyncProgressUpdated(progress));
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
    fn syncing_to_synced_on_progress_100() {
        let mut state = ConnectionState::Connecting;
        let progress = synced_progress();
        state.apply_event(&SpvEvent::SyncProgressUpdated(progress));
        assert_eq!(state, ConnectionState::Synced);
    }

    #[test]
    fn pause_from_syncing() {
        let mut state = ConnectionState::Syncing(SyncProgressView {
            percentage: 50.0,
            stage_label: "Syncing...".into(),
        });
        state.pause();
        assert_eq!(state, ConnectionState::Paused);
    }

    #[test]
    fn pause_from_synced() {
        let mut state = ConnectionState::Synced;
        state.pause();
        assert_eq!(state, ConnectionState::Paused);
    }

    #[test]
    fn pause_from_disconnected_is_noop() {
        let mut state = ConnectionState::Disconnected;
        state.pause();
        assert_eq!(state, ConnectionState::Disconnected);
    }

    #[test]
    fn resume_from_paused() {
        let mut state = ConnectionState::Paused;
        state.resume();
        assert_eq!(state, ConnectionState::Connecting);
    }

    #[test]
    fn resume_from_non_paused_is_noop() {
        let mut state = ConnectionState::Synced;
        state.resume();
        assert_eq!(state, ConnectionState::Synced);
    }

    #[test]
    fn error_from_any_state() {
        for initial in [
            ConnectionState::Disconnected,
            ConnectionState::Connecting,
            ConnectionState::Synced,
            ConnectionState::Paused,
        ] {
            let mut state = initial;
            state.apply_event(&SpvEvent::Error("connection lost".into()));
            assert!(matches!(state, ConnectionState::Error(_)));
        }
    }

    #[test]
    fn events_ignored_during_pause() {
        let mut state = ConnectionState::Paused;
        let progress = syncing_progress(50.0, ManagerId::Headers);
        state.apply_event(&SpvEvent::SyncProgressUpdated(progress));
        assert_eq!(state, ConnectionState::Paused);
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
    fn sync_stage_label_headers() {
        let view = SyncProgressView::from_progress(&syncing_progress(45.0, ManagerId::Headers));
        assert_eq!(view.stage_label, "Syncing headers... (45%)");
    }

    #[test]
    fn sync_stage_label_filters() {
        let view = SyncProgressView::from_progress(&syncing_progress(72.0, ManagerId::Filters));
        assert_eq!(view.stage_label, "Downloading filters... (72%)");
    }

    #[test]
    fn sync_stage_label_blocks() {
        let view = SyncProgressView::from_progress(&syncing_progress(90.0, ManagerId::Blocks));
        assert_eq!(view.stage_label, "Processing blocks... (90%)");
    }

    #[test]
    fn sync_stage_label_synced() {
        let view = SyncProgressView::from_progress(&synced_progress());
        assert_eq!(view.stage_label, "Synced");
    }
}
