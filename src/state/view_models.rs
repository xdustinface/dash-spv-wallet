use crate::backend::types::{TransactionDirection, TransactionInfo};

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
    pub label: Option<String>,
}

/// Convert a transaction record to a display-ready view.
pub fn format_transaction(
    tx: &TransactionInfo,
    current_height: u32,
    unit: &str,
) -> TransactionView {
    let txid_hex = tx.txid.to_string();
    let direction_label = match tx.direction {
        TransactionDirection::Outgoing => "Sent",
        TransactionDirection::Incoming => "Received",
        TransactionDirection::Internal => "Internal",
        TransactionDirection::CoinJoin => "CoinJoin",
    };
    let amount_display = format_amount(tx.amount, unit);
    let timestamp_display = format_timestamp(tx.timestamp);
    let confirmations = tx.confirmations(current_height);
    let confirmations_display = if confirmations == 0 {
        "Unconfirmed".to_string()
    } else {
        format!("{confirmations} confirmations")
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
        label: tx.label.clone(),
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
        };

        let view = format_transaction(&tx, 1000, "DASH");
        assert_eq!(view.direction_label, "Received");
        assert_eq!(view.amount_display, "+1.0 DASH");
        assert_eq!(view.confirmations_display, "7 confirmations");
        assert_eq!(view.address_short, "XqN8...BGsP");
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
        };

        let view = format_transaction(&tx, 1000, "DASH");
        assert_eq!(view.direction_label, "Sent");
        assert_eq!(view.amount_display, "-0.5 DASH");
        assert_eq!(view.confirmations_display, "Unconfirmed");
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
        };

        let view = format_transaction(&tx, 1000, "tDASH");
        assert_eq!(view.amount_display, "+1.0 tDASH");
    }

    #[test]
    fn format_transaction_no_addresses() {
        let tx = TransactionInfo {
            txid: dashcore::Txid::from_byte_array([0; 32]),
            amount: 0,
            direction: TransactionDirection::Incoming,
            transaction_type: TransactionType::Standard,
            timestamp: 0,
            height: None,
            fee: None,
            addresses: vec![],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: None,
        };

        let view = format_transaction(&tx, 0, "DASH");
        assert_eq!(view.address_short, "");
    }

    #[test]
    fn format_transaction_with_label() {
        let tx = TransactionInfo {
            txid: dashcore::Txid::from_byte_array([0xEE; 32]),
            amount: 50_000_000,
            direction: TransactionDirection::Incoming,
            transaction_type: TransactionType::Standard,
            timestamp: 1700000000,
            height: Some(500),
            fee: None,
            addresses: vec!["Xaddr".into()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: Some("coffee payment".into()),
        };

        let view = format_transaction(&tx, 1000, "DASH");
        assert_eq!(view.label, Some("coffee payment".into()));
    }

    #[test]
    fn format_transaction_without_label() {
        let tx = TransactionInfo {
            txid: dashcore::Txid::from_byte_array([0xFF; 32]),
            amount: 50_000_000,
            direction: TransactionDirection::Incoming,
            transaction_type: TransactionType::Standard,
            timestamp: 1700000000,
            height: Some(500),
            fee: None,
            addresses: vec!["Xaddr".into()],
            block_hash: None,
            is_instant_send: false,
            is_chain_locked: false,
            label: None,
        };

        let view = format_transaction(&tx, 1000, "DASH");
        assert_eq!(view.label, None);
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
}
