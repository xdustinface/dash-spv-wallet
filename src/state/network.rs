use crate::backend::events::SpvEvent;

/// Network info tracked by the UI.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkInfo {
    pub connected_peers: u32,
    pub best_height: u32,
    pub chain_tip: u32,
}

impl NetworkInfo {
    /// Apply an SPV event to update network info.
    pub fn apply_event(&mut self, event: &SpvEvent) {
        match event {
            SpvEvent::PeerConnected(_) => {
                self.connected_peers += 1;
            }
            SpvEvent::PeerDisconnected(_) => {
                self.connected_peers = self.connected_peers.saturating_sub(1);
            }
            SpvEvent::PeersUpdated { count, best_height } => {
                self.connected_peers = *count;
                self.best_height = *best_height;
            }
            SpvEvent::HeadersSynced { tip_height } | SpvEvent::SyncComplete { tip_height, .. } => {
                self.chain_tip = *tip_height;
            }
            SpvEvent::SyncProgressUpdated(progress) => {
                if let Ok(headers) = progress.headers() {
                    self.chain_tip = headers.tip_height();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::types::SyncProgress;

    #[test]
    fn default_state() {
        let info = NetworkInfo::default();
        assert_eq!(info.connected_peers, 0);
        assert_eq!(info.best_height, 0);
        assert_eq!(info.chain_tip, 0);
    }

    #[test]
    fn peer_connect_disconnect() {
        let mut info = NetworkInfo::default();

        info.apply_event(&SpvEvent::PeerConnected("1.2.3.4".into()));
        assert_eq!(info.connected_peers, 1);

        info.apply_event(&SpvEvent::PeerConnected("5.6.7.8".into()));
        assert_eq!(info.connected_peers, 2);

        info.apply_event(&SpvEvent::PeerDisconnected("1.2.3.4".into()));
        assert_eq!(info.connected_peers, 1);
    }

    #[test]
    fn disconnect_does_not_underflow() {
        let mut info = NetworkInfo::default();
        info.apply_event(&SpvEvent::PeerDisconnected("1.2.3.4".into()));
        assert_eq!(info.connected_peers, 0);
    }

    #[test]
    fn peers_updated_overrides_count() {
        let mut info = NetworkInfo {
            connected_peers: 5,
            ..Default::default()
        };

        info.apply_event(&SpvEvent::PeersUpdated {
            count: 3,
            best_height: 50000,
        });
        assert_eq!(info.connected_peers, 3);
        assert_eq!(info.best_height, 50000);
    }

    #[test]
    fn headers_synced_updates_chain_tip() {
        let mut info = NetworkInfo::default();
        info.apply_event(&SpvEvent::HeadersSynced { tip_height: 42000 });
        assert_eq!(info.chain_tip, 42000);
    }

    #[test]
    fn sync_complete_updates_chain_tip() {
        let mut info = NetworkInfo::default();
        info.apply_event(&SpvEvent::SyncComplete {
            tip_height: 50000,
            cycle: 1,
        });
        assert_eq!(info.chain_tip, 50000);
    }

    #[test]
    fn progress_without_headers_leaves_chain_tip_unchanged() {
        let mut info = NetworkInfo::default();
        let progress = SyncProgress::default();
        info.apply_event(&SpvEvent::SyncProgressUpdated(Box::new(progress)));
        assert_eq!(info.chain_tip, 0);
    }

    #[test]
    fn unrelated_events_are_ignored() {
        let mut info = NetworkInfo::default();
        info.apply_event(&SpvEvent::BalanceUpdated(Default::default()));
        assert_eq!(info, NetworkInfo::default());
    }
}
