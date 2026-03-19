mod backend;
mod state;

use dioxus::prelude::*;

fn main() {
    dioxus::launch(app);
}

fn app() -> Element {
    rsx! {
        div {
            class: "flex items-center justify-center min-h-screen bg-gray-900 text-white",
            h1 {
                class: "text-4xl font-bold text-blue-500",
                "dash-spv-ui"
            }
        }
    }
}
