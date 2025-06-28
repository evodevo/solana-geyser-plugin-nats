use {log::*, solana_sdk::pubkey::Pubkey, std::collections::HashSet};

#[derive(Default)]
pub struct TransactionSelector {
    pub mentioned_addresses: HashSet<Vec<u8>>,
    pub select_all_transactions: bool,
    pub select_all_vote_transactions: bool,
}

impl TransactionSelector {
    /// Create a selector based on the mentioned addresses
    /// To select all transactions use ["*"] or ["all"]
    /// To select all vote transactions, use ["all_votes"]
    /// To select transactions mentioning specific addresses use ["<pubkey1>", "<pubkey2>", ...]
    pub fn new(mentioned_addresses: &[String]) -> Self {
        info!("Creating TransactionSelector for addresses: {mentioned_addresses:?}");

        let select_all_transactions = mentioned_addresses
            .iter()
            .any(|key| key == "*" || key == "all");
        if select_all_transactions {
            return Self {
                mentioned_addresses: HashSet::default(),
                select_all_transactions,
                select_all_vote_transactions: true,
            };
        }
        let select_all_vote_transactions = mentioned_addresses.iter().any(|key| key == "all_votes");
        if select_all_vote_transactions {
            return Self {
                mentioned_addresses: HashSet::default(),
                select_all_transactions,
                select_all_vote_transactions: true,
            };
        }

        let mentioned_addresses = mentioned_addresses
            .iter()
            .map(|key| bs58::decode(key).into_vec().unwrap())
            .collect();

        Self {
            mentioned_addresses,
            select_all_transactions: false,
            select_all_vote_transactions: false,
        }
    }

    /// Check if a transaction is of interest.
    pub fn is_transaction_selected(
        &self,
        is_vote: bool,
        mentioned_addresses: Box<dyn Iterator<Item = &Pubkey> + '_>,
    ) -> bool {
        debug!("Transaction selector check: is_vote={}, select_all_transactions={}, select_all_vote_transactions={}", 
               is_vote, self.select_all_transactions, self.select_all_vote_transactions);

        if !self.is_enabled() {
            debug!("Transaction selector not enabled");
            return false;
        }

        if self.select_all_transactions || (self.select_all_vote_transactions && is_vote) {
            debug!(
                "Transaction selected by the rules: select_all={}, select_votes_and_is_vote={}",
                self.select_all_transactions,
                self.select_all_vote_transactions && is_vote
            );
            return true;
        }

        // Check specific addresses
        for address in mentioned_addresses {
            if self.mentioned_addresses.contains(address.as_ref()) {
                debug!("Transaction selected by address match: {address}");
                return true;
            }
        }

        debug!("Transaction not selected by any rule");
        false
    }

    /// Check if any transaction is of interest at all
    pub fn is_enabled(&self) -> bool {
        self.select_all_transactions
            || self.select_all_vote_transactions
            || !self.mentioned_addresses.is_empty()
    }
}
