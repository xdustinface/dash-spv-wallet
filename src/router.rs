use dioxus::prelude::*;

use crate::components::layout::AppLayout;
use crate::screens::dashboard::Dashboard;
use crate::screens::network_select::NetworkSelect;
use crate::screens::receive::Receive;
use crate::screens::send::Send;
use crate::screens::wallet_create::WalletCreate;
use crate::screens::wallet_import::WalletImport;

#[derive(Routable, Clone, Debug, PartialEq)]
pub enum Route {
    #[route("/")]
    NetworkSelect {},
    #[route("/wallet/create")]
    WalletCreate {},
    #[route("/wallet/import")]
    WalletImport {},
    #[layout(AppLayout)]
    #[route("/dashboard")]
    Dashboard {},
    #[route("/send")]
    Send {},
    #[route("/receive")]
    Receive {},
}
