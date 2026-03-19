use dioxus::prelude::*;

use crate::backend::mock::MockBackend;
use crate::backend::r#trait::SpvBackend;
use crate::event_bridge::use_event_bridge;

#[component]
pub fn Dashboard() -> Element {
    let backend = use_context::<Signal<MockBackend>>();

    // Start the event bridge coroutine to pipe backend events into UI state.
    use_event_bridge();

    // Auto-connect: start the backend if it is not already running.
    use_future(move || async move {
        if !backend.read().is_running() {
            let _ = backend.read().start().await;
        }
    });

    rsx! {
        div {
            class: "text-white",
            h1 {
                class: "text-2xl font-bold mb-4",
                "Dashboard"
            }
            p {
                class: "text-gray-400",
                "Wallet overview will appear here."
            }
        }
    }
}
