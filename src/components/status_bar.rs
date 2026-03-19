use dioxus::prelude::*;

#[component]
pub fn StatusBar() -> Element {
    rsx! {
        div {
            class: "flex items-center px-4 py-2 bg-gray-800 border-t border-gray-700 text-sm text-gray-400",
            span {
                class: "inline-block w-2 h-2 rounded-full bg-red-500 mr-2",
            }
            "Disconnected"
        }
    }
}
