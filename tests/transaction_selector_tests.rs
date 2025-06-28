use solana_geyser_plugin_nats::transaction_selector::TransactionSelector;
use solana_sdk::pubkey::Pubkey;

#[test]
fn test_default_selector() {
    let selector = TransactionSelector::default();
    assert!(!selector.is_enabled());
}

#[test]
fn test_select_specific_transaction() {
    let pubkey1 = Pubkey::new_unique();
    let pubkey2 = Pubkey::new_unique();

    let selector = TransactionSelector::new(&[pubkey1.to_string()]);

    assert!(selector.is_enabled());
    assert!(!selector.select_all_transactions);

    let addresses = [pubkey1];
    assert!(selector.is_transaction_selected(false, Box::new(addresses.iter())));

    let addresses = [pubkey2];
    assert!(!selector.is_transaction_selected(false, Box::new(addresses.iter())));
}

#[test]
fn test_select_all_with_wildcard() {
    let pubkey = Pubkey::new_unique();
    let selector = TransactionSelector::new(&["*".to_string()]);

    assert!(selector.is_enabled());
    assert!(selector.select_all_transactions);

    let addresses = [pubkey];
    assert!(selector.is_transaction_selected(false, Box::new(addresses.iter())));
    assert!(selector.is_transaction_selected(true, Box::new(addresses.iter())));
}

#[test]
fn test_vote_transaction_filtering() {
    let pubkey = Pubkey::new_unique();
    let selector = TransactionSelector::new(&[pubkey.to_string()]);

    let addresses = [pubkey];
    // Should select non-vote transactions that mention this address
    assert!(selector.is_transaction_selected(false, Box::new(addresses.iter())));
    // Should also select vote transactions that mention this address
    assert!(selector.is_transaction_selected(true, Box::new(addresses.iter())));
}
