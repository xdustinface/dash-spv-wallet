use dioxus::desktop::tao::event::{Event, WindowEvent};
use dioxus::desktop::{use_window, use_wry_event_handler};
use dioxus::prelude::*;

use crate::backend::dispatch::Backend;
use crate::backend::r#trait::SpvBackend;
use crate::components::dev_panel::DevPanel;
use crate::components::sidebar::Sidebar;
use crate::components::status_bar::StatusBar;
use crate::config::AppConfig;

#[component]
pub fn AppLayout() -> Element {
    let mut config = use_context::<Signal<AppConfig>>();

    // Track window resize events and update the in-memory config.
    use_wry_event_handler(move |event, _| {
        if let Event::WindowEvent {
            event: WindowEvent::Resized(size),
            ..
        } = event
        {
            let mut cfg = config.write();
            cfg.window_width = size.width;
            cfg.window_height = size.height;
        }
    });

    // Stop the backend and persist window size when the component unmounts (app close).
    let window = use_window();
    let backend = use_context::<Signal<Backend>>();
    use_drop(move || {
        if backend.read().is_running() {
            let rt = tokio::runtime::Runtime::new().expect("failed to create shutdown runtime");
            let _ = rt.block_on(backend.read().stop());
        }

        let size = window.inner_size();
        let mut cfg = config.write();
        cfg.window_width = size.width;
        cfg.window_height = size.height;
        let _ = cfg.save();
    });

    rsx! {
        div {
            class: "flex flex-col h-screen bg-surface text-foreground",

            div {
                class: "flex flex-1 min-h-0",

                Sidebar {}

                main {
                    class: "flex flex-col flex-1 min-h-0",

                    // Drag region for frameless window (macOS)
                    div {
                        class: "h-8 shrink-0 cursor-default",
                        onmousedown: move |_| {
                            dioxus::desktop::window().drag();
                        },
                    }

                    div {
                        class: "flex-1 overflow-y-auto p-6",
                        Outlet::<crate::router::Route> {}
                    }

                    DevPanel {}
                }
            }

            StatusBar {}
        }
    }
}
