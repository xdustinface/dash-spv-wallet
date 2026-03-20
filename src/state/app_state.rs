use crate::backend::types::Network;

/// Top-level application state.
#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub screen: Screen,
    pub network: Option<Network>,
    pub wallet_loaded: bool,
    pub dev_mode: bool,
}

impl AppState {
    pub fn new(dev_mode: bool) -> Self {
        Self {
            screen: Screen::NetworkSelect,
            network: None,
            wallet_loaded: false,
            dev_mode,
        }
    }

    pub fn select_network(&mut self, network: Network) {
        self.network = Some(network);
    }

    pub fn set_wallet_loaded(&mut self) {
        self.wallet_loaded = true;
        self.screen = Screen::Dashboard;
    }

    pub fn navigate(&mut self, screen: Screen) {
        self.screen = screen;
    }
}

/// Application screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    NetworkSelect,
    WalletChoice,
    WalletCreate,
    WalletImport,
    Dashboard,
    Transactions,
    Send,
    Receive,
    Settings,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state() {
        let state = AppState::new(false);
        assert_eq!(state.screen, Screen::NetworkSelect);
        assert_eq!(state.network, None);
        assert!(!state.wallet_loaded);
        assert!(!state.dev_mode);
    }

    #[test]
    fn initial_state_dev_mode() {
        let state = AppState::new(true);
        assert!(state.dev_mode);
    }

    #[test]
    fn select_network() {
        let mut state = AppState::new(false);
        state.select_network(Network::Testnet);
        assert_eq!(state.network, Some(Network::Testnet));
    }

    #[test]
    fn set_wallet_loaded_navigates_to_dashboard() {
        let mut state = AppState::new(false);
        state.set_wallet_loaded();
        assert!(state.wallet_loaded);
        assert_eq!(state.screen, Screen::Dashboard);
    }

    #[test]
    fn navigate_between_screens() {
        let mut state = AppState::new(false);
        state.set_wallet_loaded();

        state.navigate(Screen::Send);
        assert_eq!(state.screen, Screen::Send);

        state.navigate(Screen::Receive);
        assert_eq!(state.screen, Screen::Receive);

        state.navigate(Screen::Dashboard);
        assert_eq!(state.screen, Screen::Dashboard);
    }
}
