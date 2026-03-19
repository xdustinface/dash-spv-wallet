use crate::backend::types::{TransactionDirection, TransactionRecord};

/// Format a satoshi amount as DASH string (e.g., "1.23456789 DASH").
pub fn format_balance(satoshis: u64) -> String {
    let whole = satoshis / 100_000_000;
    let frac = satoshis % 100_000_000;
    if frac == 0 {
        format!("{whole}.0 DASH")
    } else {
        let frac_str = format!("{frac:08}").trim_end_matches('0').to_string();
        format!("{whole}.{frac_str} DASH")
    }
}

/// Format a signed satoshi amount (for transactions).
pub fn format_amount(satoshis: i64) -> String {
    let abs = satoshis.unsigned_abs();
    let formatted = format_balance(abs);
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

/// Display-ready transaction info.
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionView {
    pub txid_hex: String,
    pub direction_label: &'static str,
    pub amount_display: String,
    pub timestamp_display: String,
    pub confirmations_display: String,
    pub address_short: String,
    pub is_instant_send: bool,
    pub is_chain_locked: bool,
}

/// Convert a transaction record to a display-ready view.
pub fn format_transaction(tx: &TransactionRecord) -> TransactionView {
    let txid_hex = hex::encode(tx.txid);
    let direction_label = match tx.direction {
        TransactionDirection::Sent => "Sent",
        TransactionDirection::Received => "Received",
    };
    let amount_display = format_amount(tx.amount);
    let timestamp_display = format_timestamp(tx.timestamp);
    let confirmations_display = if tx.confirmations == 0 {
        "Unconfirmed".to_string()
    } else {
        format!("{} confirmations", tx.confirmations)
    };
    let address_short = tx
        .addresses
        .first()
        .map(|a| format_address_short(a))
        .unwrap_or_default();

    TransactionView {
        txid_hex,
        direction_label,
        amount_display,
        timestamp_display,
        confirmations_display,
        address_short,
        is_instant_send: tx.is_instant_send,
        is_chain_locked: tx.is_chain_locked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- format_balance --

    #[test]
    fn format_balance_zero() {
        assert_eq!(format_balance(0), "0.0 DASH");
    }

    #[test]
    fn format_balance_one_satoshi() {
        assert_eq!(format_balance(1), "0.00000001 DASH");
    }

    #[test]
    fn format_balance_one_dash() {
        assert_eq!(format_balance(100_000_000), "1.0 DASH");
    }

    #[test]
    fn format_balance_with_decimals() {
        assert_eq!(format_balance(123_456_789), "1.23456789 DASH");
    }

    #[test]
    fn format_balance_trailing_zeros_stripped() {
        assert_eq!(format_balance(150_000_000), "1.5 DASH");
    }

    #[test]
    fn format_balance_large_value() {
        assert_eq!(format_balance(2_100_000_000_000_000), "21000000.0 DASH");
    }

    // -- format_amount --

    #[test]
    fn format_amount_positive() {
        assert_eq!(format_amount(100_000_000), "+1.0 DASH");
    }

    #[test]
    fn format_amount_negative() {
        assert_eq!(format_amount(-50_000_000), "-0.5 DASH");
    }

    #[test]
    fn format_amount_zero() {
        assert_eq!(format_amount(0), "+0.0 DASH");
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

    // -- format_transaction --

    #[test]
    fn format_transaction_received() {
        let tx = TransactionRecord {
            txid: [0xAB; 32],
            amount: 100_000_000,
            direction: TransactionDirection::Received,
            timestamp: 1700000000,
            confirmations: 6,
            addresses: vec!["XqN8a73jYfHtFbEjz2XYBfrCHn6YQwBGsP".into()],
            is_instant_send: true,
            is_chain_locked: false,
        };

        let view = format_transaction(&tx);
        assert_eq!(view.direction_label, "Received");
        assert_eq!(view.amount_display, "+1.0 DASH");
        assert_eq!(view.confirmations_display, "6 confirmations");
        assert_eq!(view.address_short, "XqN8...BGsP");
        assert!(view.is_instant_send);
        assert!(!view.is_chain_locked);
        assert_eq!(view.txid_hex.len(), 64);
    }

    #[test]
    fn format_transaction_sent_unconfirmed() {
        let tx = TransactionRecord {
            txid: [0xCD; 32],
            amount: -50_000_000,
            direction: TransactionDirection::Sent,
            timestamp: 1700000000,
            confirmations: 0,
            addresses: vec!["XrecipientAddr".into()],
            is_instant_send: false,
            is_chain_locked: false,
        };

        let view = format_transaction(&tx);
        assert_eq!(view.direction_label, "Sent");
        assert_eq!(view.amount_display, "-0.5 DASH");
        assert_eq!(view.confirmations_display, "Unconfirmed");
    }

    #[test]
    fn format_transaction_no_addresses() {
        let tx = TransactionRecord {
            txid: [0; 32],
            amount: 0,
            direction: TransactionDirection::Received,
            timestamp: 0,
            confirmations: 0,
            addresses: vec![],
            is_instant_send: false,
            is_chain_locked: false,
        };

        let view = format_transaction(&tx);
        assert_eq!(view.address_short, "");
    }
}
