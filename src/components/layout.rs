use dioxus::prelude::*;

use crate::components::dev_panel::DevPanel;
use crate::components::sidebar::Sidebar;
use crate::components::status_bar::StatusBar;

#[component]
pub fn AppLayout() -> Element {
    rsx! {
        div {
            class: "flex flex-col h-screen bg-surface text-foreground",

            div {
                class: "flex flex-1 min-h-0",

                Sidebar {}

                main {
                    class: "flex flex-col flex-1 min-h-0",

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
