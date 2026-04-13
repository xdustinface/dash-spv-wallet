use crate::backend::types::{
    InputInfo, OutputInfo, OutputRole, TransactionDirection, TransactionInfo, TransactionType,
};

const SATS_PER_DASH: u64 = 100_000_000;

/// Parse a DASH amount string to satoshis.
///
/// Accepts non-negative decimal strings with up to 8 decimal places.
/// Returns `None` for empty, negative, non-numeric, or overly precise input.
pub fn parse_dash_amount(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || s.starts_with('-') {
        return None;
    }

    let (whole_str, frac_str) = match s.split_once('.') {
        Some((w, f)) => {
            if f.len() > 8 {
                return None;
            }
            (w, f)
        }
        None => (s, ""),
    };

    let whole: u64 = whole_str.parse().ok()?;
    let frac: u64 = if frac_str.is_empty() {
        0
    } else {
        let padded = format!("{frac_str:0<8}");
        padded.parse().ok()?
    };

    whole.checked_mul(SATS_PER_DASH)?.checked_add(frac)
}

/// Format a satoshi amount as a currency string (e.g., "1.23456789 DASH").
pub fn format_balance(satoshis: u64, unit: &str) -> String {
    let whole = satoshis / 100_000_000;
    let frac = satoshis % 100_000_000;
    if frac == 0 {
        format!("{whole}.0 {unit}")
    } else {
        let frac_str = format!("{frac:08}").trim_end_matches('0').to_string();
        format!("{whole}.{frac_str} {unit}")
    }
}

/// Format a signed satoshi amount (for transactions).
pub fn format_amount(satoshis: i64, unit: &str) -> String {
    let abs = satoshis.unsigned_abs();
    let formatted = format_balance(abs, unit);
    if satoshis < 0 {
        format!("-{formatted}")
    } else {
        format!("+{formatted}")
    }
}

/// Truncate an address for list display (e.g., "Xq3k...7f2a").
pub fn format_address_short(address: &str) -> String {
    if address.len() <= 12 {
        return address.to_string();
    }
    let prefix = &address[..4];
    let suffix = &address[address.len() - 4..];
    format!("{prefix}...{suffix}")
}

/// Format a peer count for display.
pub fn format_peer_count(count: u32) -> String {
    match count {
        0 => "No peers".to_string(),
        1 => "1 peer".to_string(),
        n => format!("{n} peers"),
    }
}

/// Format a Unix timestamp as a human-readable relative or absolute date.
pub fn format_timestamp(timestamp: u64) -> String {
    if timestamp == 0 {
        return "Pending".to_string();
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if timestamp > now {
        return "just now".to_string();
    }

    let elapsed = now - timestamp;

    if elapsed < 60 {
        return "just now".to_string();
    }
    if elapsed < 3600 {
        let mins = elapsed / 60;
        return if mins == 1 {
            "1 min ago".to_string()
        } else {
            format!("{mins} min ago")
        };
    }
    if elapsed < 86400 {
        let hours = elapsed / 3600;
        return if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{hours} hours ago")
        };
    }
    if elapsed < 86400 * 30 {
        let days = elapsed / 86400;
        return if days == 1 {
            "1 day ago".to_string()
        } else {
            format!("{days} days ago")
        };
    }

    // For older timestamps, show a date
    let days_total = timestamp / 86400;
    let year = 1970 + (days_total / 365);
    let month = ((days_total % 365) / 30) + 1;
    let day = (days_total % 30) + 1;
    format!("{year}-{month:02}-{day:02}")
}

/// Middle-truncate an address for responsive display.
pub fn format_address_responsive(address: &str, max_chars: usize) -> String {
    if address.len() <= max_chars {
        return address.to_string();
    }
    let keep = max_chars.saturating_sub(3) / 2;
    let prefix = &address[..keep];
    let suffix = &address[address.len() - keep..];
    format!("{prefix}...{suffix}")
}

/// Format a Unix timestamp as an absolute UTC date/time string.
pub fn format_timestamp_absolute(timestamp: u64) -> String {
    if timestamp == 0 {
        return "Pending".to_string();
    }

    let mut remaining_days = (timestamp / 86400) as i64;
    let day_seconds = timestamp % 86400;
    let hours = day_seconds / 3600;
    let minutes = (day_seconds % 3600) / 60;
    let seconds = day_seconds % 60;

    // Compute year and day-of-year from days since epoch
    let mut year: i64 = 1970;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    // Compute month and day from day-of-year
    let leap = is_leap_year(year);
    let month_days: [i64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    for (i, &days) in month_days.iter().enumerate() {
        if remaining_days < days {
            month = i + 1;
            break;
        }
        remaining_days -= days;
    }
    let day = remaining_days + 1;

    format!("{year}-{month:02}-{day:02} {hours:02}:{minutes:02}:{seconds:02}")
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Check whether a transaction matches a search query.
///
/// Matches against txid, addresses, and formatted amount (case-insensitive).
pub fn matches_search(tx: &TransactionInfo, query: &str, unit: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let q = query.to_lowercase();
    tx.txid.to_string().to_lowercase().contains(&q)
        || tx.addresses.iter().any(|a| a.to_lowercase().contains(&q))
        || format_balance(tx.amount.unsigned_abs(), unit)
            .to_lowercase()
            .contains(&q)
        || tx
            .label
            .as_ref()
            .is_some_and(|l| l.to_lowercase().contains(&q))
}

struct DirectionStyle {
    label: &'static str,
    icon: &'static str,
    icon_class: &'static str,
    border_class: &'static str,
    amount_class: &'static str,
}

/// Display-ready transaction info.
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionView {
    pub txid_hex: String,
    pub direction_label: &'static str,
    pub direction_icon: &'static str,
    pub direction_icon_class: &'static str,
    pub border_class: &'static str,
    pub amount_class: &'static str,
    pub type_badge: Option<&'static str>,
    pub amount_display: String,
    pub timestamp_display: String,
    pub absolute_date: String,
    pub confirmations_display: String,
    pub fee_display: Option<String>,
    pub block_hash_display: Option<String>,
    pub height_display: Option<String>,
    pub is_instant_send: bool,
    pub is_chain_locked: bool,
}

/// Derive the type badge label for non-standard transaction types.
///
/// Returns `None` when the badge would be redundant with the direction label
/// (e.g., CoinJoin direction already shows "CoinJoin").
fn type_badge_label(
    tx_type: TransactionType,
    direction: TransactionDirection,
) -> Option<&'static str> {
    match tx_type {
        TransactionType::Standard => None,
        TransactionType::CoinJoin if direction == TransactionDirection::CoinJoin => None,
        TransactionType::CoinJoin => Some("CoinJoin"),
        TransactionType::Coinbase => Some("Coinbase"),
        TransactionType::AssetLock => Some("AssetLock"),
        TransactionType::AssetUnlock => Some("AssetUnlock"),
        TransactionType::ProviderRegistration => Some("ProReg"),
        TransactionType::ProviderUpdateRegistrar => Some("ProUpReg"),
        TransactionType::ProviderUpdateService => Some("ProUpServ"),
        TransactionType::ProviderUpdateRevocation => Some("ProUpRev"),
        TransactionType::Ignored => None,
    }
}

/// Convert a transaction record to a display-ready view.
pub fn format_transaction(
    tx: &TransactionInfo,
    current_height: u32,
    unit: &str,
) -> TransactionView {
    let txid_hex = tx.txid.to_string();

    let style = match tx.direction {
        TransactionDirection::Incoming => DirectionStyle {
            label: "Received",
            icon: "\u{25bc}",
            icon_class: "text-success text-lg flex-shrink-0",
            border_class: "border-success",
            amount_class: "text-success font-medium",
        },
        TransactionDirection::Outgoing => DirectionStyle {
            label: "Sent",
            icon: "\u{25b2}",
            icon_class: "text-error text-lg flex-shrink-0",
            border_class: "border-error",
            amount_class: "text-error font-medium",
        },
        TransactionDirection::Internal => DirectionStyle {
            label: "Internal",
            icon: "\u{21c4}",
            icon_class: "text-muted text-lg flex-shrink-0",
            border_class: "border-muted",
            amount_class: "text-muted font-medium",
        },
        TransactionDirection::CoinJoin => DirectionStyle {
            label: "CoinJoin",
            icon: "\u{21cb}",
            icon_class: "text-muted text-lg flex-shrink-0",
            border_class: "border-muted",
            amount_class: "text-muted font-medium",
        },
    };

    let type_badge = type_badge_label(tx.transaction_type, tx.direction);

    let amount_display = format_amount(tx.amount, unit);
    let timestamp_display = format_timestamp(tx.timestamp);
    let absolute_date = format_timestamp_absolute(tx.timestamp);
    let confirmations = tx.confirmations(current_height);
    let confirmations_display = if confirmations == 0 {
        "Unconfirmed".to_string()
    } else {
        format!("{confirmations} confirmations")
    };
    let fee_display = tx.fee.map(|f| format_balance(f, unit));
    let block_hash_display = tx.block_hash.map(|h| h.to_string());
    let height_display = tx.height.map(|h| h.to_string());

    TransactionView {
        txid_hex,
        direction_label: style.label,
        direction_icon: style.icon,
        direction_icon_class: style.icon_class,
        border_class: style.border_class,
        amount_class: style.amount_class,
        type_badge,
        amount_display,
        timestamp_display,
        absolute_date,
        confirmations_display,
        fee_display,
        block_hash_display,
        height_display,
        is_instant_send: tx.is_instant_send,
        is_chain_locked: tx.is_chain_locked,
    }
}

/// Display-ready transaction input.
#[derive(Debug, Clone, PartialEq)]
pub struct InputView {
    pub index: u32,
    pub address_short: String,
    pub amount_display: String,
}

/// Display-ready transaction output.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputView {
    pub index: u32,
    pub address_short: String,
    pub amount_display: String,
    pub role_label: &'static str,
    pub role_color: &'static str,
}

/// Format a transaction input for display.
pub fn format_input(input: &InputInfo, unit: &str) -> InputView {
    InputView {
        index: input.index,
        address_short: format_address_short(&input.address),
        amount_display: format_balance(input.value, unit),
    }
}

/// Format a transaction output for display.
pub fn format_output(output: &OutputInfo, unit: &str) -> OutputView {
    let (role_label, role_color) = role_display(output.role);
    OutputView {
        index: output.index,
        address_short: format_address_short(&output.address),
        amount_display: format_balance(output.value, unit),
        role_label,
        role_color,
    }
}

/// The number of inputs/outputs shown before a "Show all" toggle appears.
pub const COLLAPSE_THRESHOLD: usize = 5;

/// Return the visible input views and whether more are hidden.
///
/// When `expanded` is false and the input count exceeds `threshold`, only
/// the first `threshold` inputs are included and `has_more` is `true`.
pub fn visible_input_views(
    inputs: &[InputInfo],
    expanded: bool,
    threshold: usize,
    unit: &str,
) -> (Vec<InputView>, bool) {
    let has_more = inputs.len() > threshold;
    let count = if expanded || !has_more {
        inputs.len()
    } else {
        threshold
    };
    let views = inputs
        .iter()
        .take(count)
        .map(|i| format_input(i, unit))
        .collect();
    (views, has_more)
}

/// Return the visible output views and whether more are hidden.
///
/// When `expanded` is false and the output count exceeds `threshold`, only
/// the first `threshold` outputs are included and `has_more` is `true`.
pub fn visible_output_views(
    outputs: &[OutputInfo],
    expanded: bool,
    threshold: usize,
    unit: &str,
) -> (Vec<OutputView>, bool) {
    let has_more = outputs.len() > threshold;
    let count = if expanded || !has_more {
        outputs.len()
    } else {
        threshold
    };
    let views = outputs
        .iter()
        .take(count)
        .map(|o| format_output(o, unit))
        .collect();
    (views, has_more)
}

/// Map an `OutputRole` to a human-readable label and Tailwind color class.
fn role_display(role: OutputRole) -> (&'static str, &'static str) {
    match role {
        OutputRole::Received => ("Received", "bg-success"),
        OutputRole::Change => ("Change", "bg-muted"),
        OutputRole::Sent => ("Sent", "bg-error"),
        OutputRole::Unspendable => ("Unspendable", "bg-disabled"),
    }
}

#[cfg(test)]
mod tests {
    use dashcore::hashes::Hash;

    use crate::backend::types::TransactionType;

    use super::*;

    // -- format_balance --

    #[test]
    fn format_balance_zero() {
        assert_eq!(format_balance(0, "DASH"), "0.0 DASH");
    }

    #[test]
    fn format_balance_one_satoshi() {
        assert_eq!(format_balance(1, "DASH"), "0.00000001 DASH");
    }

    #[test]
    fn format_balance_one_dash() {
        assert_eq!(format_balance(100_000_000, "DASH"), "1.0 DASH");
    }

    #[test]
    fn format_balance_with_decimals() {
        assert_eq!(format_balance(123_456_789, "DASH"), "1.23456789 DASH");
    }

    #[test]
    fn format_balance_trailing_zeros_stripped() {
        assert_eq!(format_balance(150_000_000, "DASH"), "1.5 DASH");
    }

    #[test]
    fn format_balance_large_value() {
        assert_eq!(
            format_balance(2_100_000_000_000_000, "DASH"),
            "21000000.0 DASH"
        );
    }

    #[test]
    fn format_balance_testnet_unit() {
        assert_eq!(format_balance(100_000_000, "tDASH"), "1.0 tDASH");
        assert_eq!(format_balance(50_000_000, "tDASH"), "0.5 tDASH");
    }

    // -- format_amount --

    #[test]
    fn format_amount_positive() {
        assert_eq!(format_amount(100_000_000, "DASH"), "+1.0 DASH");
    }

    #[test]
    fn format_amount_negative() {
        assert_eq!(format_amount(-50_000_000, "DASH"), "-0.5 DASH");
    }

    #[test]
    fn format_amount_zero() {
        assert_eq!(format_amount(0, "DASH"), "+0.0 DASH");
    }

    #[test]
    fn format_amount_testnet_unit() {
        assert_eq!(format_amount(100_000_000, "tDASH"), "+1.0 tDASH");
        assert_eq!(format_amount(-50_000_000, "tDASH"), "-0.5 tDASH");
    }

    // -- format_address_short --

    #[test]
    fn format_address_short_long() {
        assert_eq!(
            format_address_short("XqN8a73jYfHtFbEjz2XYBfrCHn6YQwBGsP"),
            "XqN8...BGsP",
        );
    }

    #[test]
    fn format_address_short_already_short() {
        assert_eq!(format_address_short("XqN8a73j"), "XqN8a73j");
    }

    #[test]
    fn format_address_short_empty() {
        assert_eq!(format_address_short(""), "");
    }

    // -- format_peer_count --

    #[test]
    fn format_peer_count_zero() {
        assert_eq!(format_peer_count(0), "No peers");
    }

    #[test]
    fn format_peer_count_one() {
        assert_eq!(format_peer_count(1), "1 peer");
    }

    #[test]
    fn format_peer_count_many() {
        assert_eq!(format_peer_count(5), "5 peers");
    }

    // -- format_timestamp --

    #[test]
    fn format_timestamp_recent() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(format_timestamp(now - 30), "just now");
    }

    #[test]
    fn format_timestamp_minutes_ago() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(format_timestamp(now - 300), "5 min ago");
    }

    #[test]
    fn format_timestamp_one_hour_ago() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(format_timestamp(now - 3600), "1 hour ago");
    }

    #[test]
    fn format_timestamp_hours_ago() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(format_timestamp(now - 7200), "2 hours ago");
    }

    #[test]
    fn format_timestamp_days_ago() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(format_timestamp(now - 172800), "2 days ago");
    }

    #[test]
    fn format_timestamp_old() {
        // 2023-11-14 (approx)
        assert!(format_timestamp(1700000000).contains("2023"));
    }

    #[test]
    fn format_timestamp_future() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(format_timestamp(now + 1000), "just now");
    }

    #[test]
    fn format_timestamp_zero_is_pending() {
        assert_eq!(format_timestamp(0), "Pending");
    }

    // -- format_transaction --

    #[test]
    fn format_transaction_received() {
        let tx = TransactionInfo {
            txid: dashcore::Txid::from_byte_array([0xAB; 32]),
            amount: 100_000_000,
            direction: TransactionDirection::Incoming,
            transaction_type: TransactionType::Standard,
            timestamp: 1700000000,
            height: Some(994),
            fee: None,
            addresses: vec!["XqN8a73jYfHtFbEjz2XYBfrCHn6YQwBGsP".into()],
            block_hash: None,
            is_instant_send: true,
            is_chain_locked: false,
            label: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
        };

        let view = format_transaction(&tx, 1000, "DASH");
        assert_eq!(view.direction_label, "Received");
        assert_eq!(view.direction_icon, "▼");
        assert_eq!(
            view.direction_icon_class,
            "text-success text-lg flex-shrink-0"
        );
        assert_eq!(view.border_class, "border-success");
        assert_eq!(view.amount_class, "text-success font-medium");
        assert_eq!(view.type_badge, None);
        assert_eq!(view.amount_display, "+1.0 DASH");
        assert_eq!(view.confirmations_display, "7 confirmations");
        assert_eq!(view.absolute_date, "2023-11-14 22:13:20");
        assert_eq!(view.fee_display, None);
        assert_eq!(view.block_hash_display, None);
        assert_eq!(view.height_display, Some("994".to_string()));
        assert!(view.is_instant_send);
        assert!(!view.is_chain_locked);
        assert_eq!(view.txid_hex.len(), 64);
    }

    #[test]
    fn format_transaction_sent_unconfirmed() {
        let tx = TransactionInfo {
            txid: dashcore::Txid::from_byte_array([0xCD; 32]),
            amount: -50_000_000,
            direction: TransactionDirection::Outgoing,
            transaction_type: TransactionType::Standard,
            timestamp: 1700000000,
            height: None,
            fee: Some(226),
            addresses: vec!["XrecipientAddr".into()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
        };

        let view = format_transaction(&tx, 1000, "DASH");
        assert_eq!(view.direction_label, "Sent");
        assert_eq!(view.direction_icon, "▲");
        assert_eq!(
            view.direction_icon_class,
            "text-error text-lg flex-shrink-0"
        );
        assert_eq!(view.border_class, "border-error");
        assert_eq!(view.amount_class, "text-error font-medium");
        assert_eq!(view.type_badge, None);
        assert_eq!(view.amount_display, "-0.5 DASH");
        assert_eq!(view.confirmations_display, "Unconfirmed");
        assert_eq!(view.fee_display, Some("0.00000226 DASH".to_string()));
        assert_eq!(view.height_display, None);
    }

    #[test]
    fn format_transaction_testnet_unit() {
        let tx = TransactionInfo {
            txid: dashcore::Txid::from_byte_array([0xAB; 32]),
            amount: 100_000_000,
            direction: TransactionDirection::Incoming,
            transaction_type: TransactionType::Standard,
            timestamp: 1700000000,
            height: Some(994),
            fee: None,
            addresses: vec!["yAddr123".into()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
        };

        let view = format_transaction(&tx, 1000, "tDASH");
        assert_eq!(view.amount_display, "+1.0 tDASH");
    }

    #[test]
    fn format_transaction_internal() {
        let tx = TransactionInfo {
            txid: dashcore::Txid::from_byte_array([0xDD; 32]),
            amount: 0,
            direction: TransactionDirection::Internal,
            transaction_type: TransactionType::Standard,
            timestamp: 1700000000,
            height: Some(500),
            fee: Some(226),
            addresses: vec!["yInternalAddr".into()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
        };

        let view = format_transaction(&tx, 1000, "DASH");
        assert_eq!(view.direction_label, "Internal");
        assert_eq!(view.direction_icon, "⇄");
        assert_eq!(
            view.direction_icon_class,
            "text-muted text-lg flex-shrink-0"
        );
        assert_eq!(view.border_class, "border-muted");
        assert_eq!(view.amount_class, "text-muted font-medium");
        assert_eq!(view.type_badge, None);
    }

    #[test]
    fn format_transaction_coinjoin() {
        let tx = TransactionInfo {
            txid: dashcore::Txid::from_byte_array([0xEE; 32]),
            amount: -25_000_000,
            direction: TransactionDirection::CoinJoin,
            transaction_type: TransactionType::CoinJoin,
            timestamp: 1700000000,
            height: Some(500),
            fee: Some(100),
            addresses: vec!["yMixAddr".into()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
        };

        let view = format_transaction(&tx, 1000, "DASH");
        assert_eq!(view.direction_label, "CoinJoin");
        assert_eq!(view.direction_icon, "⇋");
        assert_eq!(
            view.direction_icon_class,
            "text-muted text-lg flex-shrink-0"
        );
        assert_eq!(view.border_class, "border-muted");
        assert_eq!(view.type_badge, None);
    }

    #[test]
    fn format_transaction_type_badges() {
        let make_tx = |tx_type: TransactionType| TransactionInfo {
            txid: dashcore::Txid::from_byte_array([0xFF; 32]),
            amount: 100_000_000,
            direction: TransactionDirection::Incoming,
            transaction_type: tx_type,
            timestamp: 1700000000,
            height: Some(500),
            fee: None,
            addresses: vec![],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
        };

        assert_eq!(
            format_transaction(&make_tx(TransactionType::Standard), 1000, "DASH").type_badge,
            None
        );
        assert_eq!(
            format_transaction(&make_tx(TransactionType::Coinbase), 1000, "DASH").type_badge,
            Some("Coinbase")
        );
        assert_eq!(
            format_transaction(&make_tx(TransactionType::AssetLock), 1000, "DASH").type_badge,
            Some("AssetLock")
        );
        assert_eq!(
            format_transaction(&make_tx(TransactionType::AssetUnlock), 1000, "DASH").type_badge,
            Some("AssetUnlock")
        );
        assert_eq!(
            format_transaction(
                &make_tx(TransactionType::ProviderRegistration),
                1000,
                "DASH"
            )
            .type_badge,
            Some("ProReg")
        );
        assert_eq!(
            format_transaction(
                &make_tx(TransactionType::ProviderUpdateRegistrar),
                1000,
                "DASH"
            )
            .type_badge,
            Some("ProUpReg")
        );
        assert_eq!(
            format_transaction(
                &make_tx(TransactionType::ProviderUpdateService),
                1000,
                "DASH"
            )
            .type_badge,
            Some("ProUpServ")
        );
        assert_eq!(
            format_transaction(
                &make_tx(TransactionType::ProviderUpdateRevocation),
                1000,
                "DASH"
            )
            .type_badge,
            Some("ProUpRev")
        );
        assert_eq!(
            format_transaction(&make_tx(TransactionType::Ignored), 1000, "DASH").type_badge,
            None
        );
    }

    // -- parse_dash_amount --

    #[test]
    fn parse_dash_amount_zero() {
        assert_eq!(parse_dash_amount("0"), Some(0));
    }

    #[test]
    fn parse_dash_amount_one() {
        assert_eq!(parse_dash_amount("1"), Some(100_000_000));
    }

    #[test]
    fn parse_dash_amount_decimal() {
        assert_eq!(parse_dash_amount("1.5"), Some(150_000_000));
    }

    #[test]
    fn parse_dash_amount_one_satoshi() {
        assert_eq!(parse_dash_amount("0.00000001"), Some(1));
    }

    #[test]
    fn parse_dash_amount_empty() {
        assert_eq!(parse_dash_amount(""), None);
    }

    #[test]
    fn parse_dash_amount_non_numeric() {
        assert_eq!(parse_dash_amount("abc"), None);
    }

    #[test]
    fn parse_dash_amount_negative() {
        assert_eq!(parse_dash_amount("-1"), None);
    }

    #[test]
    fn parse_dash_amount_too_many_decimals() {
        assert_eq!(parse_dash_amount("1.123456789"), None);
    }

    // -- matches_search --

    fn sample_tx() -> TransactionInfo {
        TransactionInfo {
            txid: dashcore::Txid::from_byte_array([0xAB; 32]),
            amount: 150_000_000,
            direction: TransactionDirection::Incoming,
            transaction_type: TransactionType::Standard,
            timestamp: 1700000000,
            height: Some(1000),
            fee: None,
            addresses: vec!["XqN8a73jYfHtFbEjz2XYBfrCHn6YQwBGsP".into()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    #[test]
    fn matches_search_empty_query() {
        assert!(matches_search(&sample_tx(), "", "DASH"));
    }

    #[test]
    fn matches_search_txid_partial() {
        let tx = sample_tx();
        let txid_prefix = &tx.txid.to_string()[..8];
        assert!(matches_search(&tx, txid_prefix, "DASH"));
    }

    #[test]
    fn matches_search_address() {
        assert!(matches_search(&sample_tx(), "XqN8a73j", "DASH"));
    }

    #[test]
    fn matches_search_amount() {
        assert!(matches_search(&sample_tx(), "1.5", "DASH"));
    }

    #[test]
    fn matches_search_no_match() {
        assert!(!matches_search(&sample_tx(), "zzz_no_match_zzz", "DASH"));
    }

    #[test]
    fn matches_search_by_label() {
        let mut tx = sample_tx();
        tx.label = Some("coffee payment".into());
        assert!(matches_search(&tx, "coffee", "DASH"));
        assert!(!matches_search(&tx, "groceries", "DASH"));
    }

    #[test]
    fn matches_search_by_unit() {
        assert!(matches_search(&sample_tx(), "tDASH", "tDASH"));
        assert!(!matches_search(&sample_tx(), "tDASH", "DASH"));
    }

    // -- format_address_responsive --

    #[test]
    fn format_address_responsive_fits() {
        let addr = "yAddr123";
        assert_eq!(format_address_responsive(addr, 20), "yAddr123");
    }

    #[test]
    fn format_address_responsive_truncated() {
        let addr = "yj12i9j58asdfghjklqwertyuiop";
        let result = format_address_responsive(addr, 15);
        assert!(result.contains("..."));
        assert!(result.len() <= 15);
        assert!(result.starts_with(&addr[..6]));
        assert!(result.ends_with(&addr[addr.len() - 6..]));
    }

    #[test]
    fn format_address_responsive_exact_length() {
        let addr = "yAddr1234567890";
        assert_eq!(format_address_responsive(addr, 15), addr);
    }

    // -- format_timestamp_absolute --

    #[test]
    fn format_timestamp_absolute_zero() {
        assert_eq!(format_timestamp_absolute(0), "Pending");
    }

    #[test]
    fn format_timestamp_absolute_known_value() {
        // 2023-11-14 22:13:20 UTC
        assert_eq!(format_timestamp_absolute(1700000000), "2023-11-14 22:13:20",);
    }

    #[test]
    fn format_timestamp_absolute_epoch() {
        assert_eq!(format_timestamp_absolute(1), "1970-01-01 00:00:01");
    }

    #[test]
    fn format_timestamp_absolute_leap_year() {
        // 2024-02-29 00:00:00 UTC = 1709164800
        assert_eq!(format_timestamp_absolute(1709164800), "2024-02-29 00:00:00",);
    }

    // -- format_input / format_output --

    #[test]
    fn format_input_displays_correctly() {
        let input = InputInfo {
            index: 0,
            value: 150_000_000,
            address: "XqN8a73jYfHtFbEjz2XYBfrCHn6YQwBGsP".into(),
        };
        let view = format_input(&input, "DASH");
        assert_eq!(view.index, 0);
        assert_eq!(view.address_short, "XqN8...BGsP");
        assert_eq!(view.amount_display, "1.5 DASH");
    }

    #[test]
    fn format_output_received() {
        let output = OutputInfo {
            index: 0,
            value: 100_000_000,
            address: "XqN8a73jYfHtFbEjz2XYBfrCHn6YQwBGsP".into(),
            role: OutputRole::Received,
        };
        let view = format_output(&output, "DASH");
        assert_eq!(view.index, 0);
        assert_eq!(view.address_short, "XqN8...BGsP");
        assert_eq!(view.amount_display, "1.0 DASH");
        assert_eq!(view.role_label, "Received");
        assert_eq!(view.role_color, "bg-success");
    }

    #[test]
    fn visible_input_views_all_shown_when_under_threshold() {
        let inputs: Vec<InputInfo> = (0..3)
            .map(|i| InputInfo {
                index: i,
                value: 1_000,
                address: String::new(),
            })
            .collect();
        let (views, has_more) = visible_input_views(&inputs, false, 5, "DASH");
        assert_eq!(views.len(), 3);
        assert!(!has_more);
    }

    #[test]
    fn visible_input_views_truncated_when_collapsed() {
        let inputs: Vec<InputInfo> = (0..8)
            .map(|i| InputInfo {
                index: i,
                value: 1_000,
                address: String::new(),
            })
            .collect();
        let (views, has_more) = visible_input_views(&inputs, false, 5, "DASH");
        assert_eq!(views.len(), 5);
        assert!(has_more);
    }

    #[test]
    fn visible_input_views_all_shown_when_expanded() {
        let inputs: Vec<InputInfo> = (0..8)
            .map(|i| InputInfo {
                index: i,
                value: 1_000,
                address: String::new(),
            })
            .collect();
        let (views, has_more) = visible_input_views(&inputs, true, 5, "DASH");
        assert_eq!(views.len(), 8);
        assert!(has_more);
    }

    #[test]
    fn visible_output_views_all_shown_when_under_threshold() {
        let outputs: Vec<OutputInfo> = (0..3)
            .map(|i| OutputInfo {
                index: i,
                value: 500,
                address: String::new(),
                role: OutputRole::Received,
            })
            .collect();
        let (views, has_more) = visible_output_views(&outputs, false, 5, "DASH");
        assert_eq!(views.len(), 3);
        assert!(!has_more);
    }

    #[test]
    fn visible_output_views_truncated_when_collapsed() {
        let outputs: Vec<OutputInfo> = (0..7)
            .map(|i| OutputInfo {
                index: i,
                value: 500,
                address: String::new(),
                role: OutputRole::Received,
            })
            .collect();
        let (views, has_more) = visible_output_views(&outputs, false, 5, "DASH");
        assert_eq!(views.len(), 5);
        assert!(has_more);
    }

    #[test]
    fn visible_output_views_all_shown_when_expanded() {
        let outputs: Vec<OutputInfo> = (0..7)
            .map(|i| OutputInfo {
                index: i,
                value: 500,
                address: String::new(),
                role: OutputRole::Received,
            })
            .collect();
        let (views, has_more) = visible_output_views(&outputs, true, 5, "DASH");
        assert_eq!(views.len(), 7);
        assert!(has_more);
    }

    #[test]
    fn format_output_all_roles() {
        let make = |role| OutputInfo {
            index: 0,
            value: 0,
            address: String::new(),
            role,
        };
        let change = format_output(&make(OutputRole::Change), "DASH");
        assert_eq!(change.role_label, "Change");
        assert_eq!(change.role_color, "bg-muted");

        let sent = format_output(&make(OutputRole::Sent), "DASH");
        assert_eq!(sent.role_label, "Sent");
        assert_eq!(sent.role_color, "bg-error");

        let unspendable = format_output(&make(OutputRole::Unspendable), "DASH");
        assert_eq!(unspendable.role_label, "Unspendable");
        assert_eq!(unspendable.role_color, "bg-disabled");
    }
}
