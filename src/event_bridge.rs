use dioxus::prelude::*;

use crate::backend::dispatch::Backend;
use crate::backend::r#trait::SpvBackend;
use crate::state::connection::ConnectionState;
use crate::state::dev_log::DevLog;
use crate::state::network::NetworkInfo;
use crate::state::wallet::WalletState;

/// Spawns a coroutine that bridges async backend events into reactive UI signals.
///
/// Call this once from a component that has all the required context signals.
/// The coroutine subscribes to the backend event channel and updates
/// `ConnectionState`, `WalletState`, `NetworkInfo`, and `DevLog`
/// on each received event.
pub fn use_event_bridge() {
    let backend = use_context::<Signal<Backend>>();
    let mut connection = use_context::<Signal<ConnectionState>>();
    let mut wallet = use_context::<Signal<WalletState>>();
    let mut network_info = use_context::<Signal<NetworkInfo>>();
    let mut dev_log = use_context::<Signal<DevLog>>();

    use_coroutine(move |_: UnboundedReceiver<()>| async move {
        let mut rx = backend.read().subscribe_events();

        loop {
            match rx.recv().await {
                Ok(event) => {
                    connection.write().apply_event(&event);
                    wallet.write().apply_event(&event);
                    network_info.write().apply_event(&event);
                    dev_log.write().push(&event);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::error!("Event bridge lost {n} events — UI state may be inconsistent");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });
}
