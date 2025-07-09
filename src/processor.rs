use {
    crate::{
        config::TransactionFilterConfig,
        connection::{ConnectionManager, NatsMessage},
        serializer::{SerializationError, TransactionSerializer},
        transaction_selector::TransactionSelector,
    },
    agave_geyser_plugin_interface::geyser_plugin_interface::{
        ReplicaTransactionInfo, ReplicaTransactionInfoV2, ReplicaTransactionInfoVersions,
    },
    log::{debug, info},
    serde_json,
    std::sync::Arc,
    thiserror::Error,
};

#[derive(Error, Debug)]
pub enum ProcessingError {
    #[error("Connection error: {0}")]
    Connection(#[from] crate::connection::ConnectionError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] SerializationError),

    #[error("Transaction filtering failed: {msg}")]
    FilteringFailed { msg: String },

    #[error("Transaction processor not initialized: {msg}")]
    NotInitialized { msg: String },
}

pub struct TransactionProcessor {
    connection_manager: Arc<ConnectionManager>,
    transaction_selector: TransactionSelector,
    subject: String,
}

impl TransactionProcessor {
    /// Create a new transaction processor
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        filter_config: &TransactionFilterConfig,
        subject: String,
    ) -> Self {
        let transaction_selector = Self::create_transaction_selector(filter_config);

        info!("Transaction processor created with subject: {subject}");
        debug!("Filter configuration: {filter_config:?}");

        Self {
            connection_manager,
            transaction_selector,
            subject,
        }
    }

    /// Create transaction selector from filter configuration
    fn create_transaction_selector(filter_config: &TransactionFilterConfig) -> TransactionSelector {
        if filter_config.select_all_transactions {
            TransactionSelector::new(&["*".to_string()])
        } else if filter_config.select_vote_transactions
            && filter_config.mentioned_addresses.is_empty()
        {
            TransactionSelector::new(&["all_votes".to_string()])
        } else if !filter_config.mentioned_addresses.is_empty() {
            TransactionSelector::new(&filter_config.mentioned_addresses)
        } else {
            // Default: select all non-vote transactions
            TransactionSelector::new(&["*".to_string()])
        }
    }

    /// Process a transaction
    pub fn process_transaction(
        &self,
        transaction_info: ReplicaTransactionInfoVersions,
        slot: u64,
    ) -> Result<(), ProcessingError> {
        match transaction_info {
            ReplicaTransactionInfoVersions::V0_0_2(transaction_info) => {
                self.process_transaction_v2(transaction_info, slot)
            }
            ReplicaTransactionInfoVersions::V0_0_1(transaction_info) => {
                self.process_transaction_v1(transaction_info, slot)
            }
        }
    }

    /// Process a V2 transaction
    fn process_transaction_v2(
        &self,
        transaction_info: &ReplicaTransactionInfoV2,
        slot: u64,
    ) -> Result<(), ProcessingError> {
        debug!(
            "Processing transaction V2: signature={}, is_vote={}, slot={}",
            transaction_info.signature, transaction_info.is_vote, slot
        );

        // Apply transaction filtering
        if !self.should_process_transaction(
            transaction_info.is_vote,
            transaction_info.transaction.message().account_keys().iter(),
        ) {
            debug!("Transaction filtered out: {}", transaction_info.signature);
            return Ok(());
        }

        info!(
            "Processing non-vote transaction: {}",
            transaction_info.signature
        );

        // Serialize and send transaction
        self.serialize_and_send_v2(transaction_info, slot)
    }

    /// Process a V1 transaction
    fn process_transaction_v1(
        &self,
        transaction_info: &ReplicaTransactionInfo,
        slot: u64,
    ) -> Result<(), ProcessingError> {
        debug!(
            "Processing transaction V1: signature={}, is_vote={}, slot={}",
            transaction_info.signature, transaction_info.is_vote, slot
        );

        // Apply transaction filtering
        if !self.should_process_transaction(
            transaction_info.is_vote,
            transaction_info.transaction.message().account_keys().iter(),
        ) {
            debug!("Transaction filtered out: {}", transaction_info.signature);
            return Ok(());
        }

        info!(
            "Processing non-vote transaction: {}",
            transaction_info.signature
        );

        // Serialize and send transaction
        self.serialize_and_send_v1(transaction_info, slot)
    }

    /// Serialize and send V2 transaction
    fn serialize_and_send_v2(
        &self,
        transaction_info: &ReplicaTransactionInfoV2,
        slot: u64,
    ) -> Result<(), ProcessingError> {
        // Serialize transaction
        let transaction_value =
            TransactionSerializer::serialize_transaction_v2(transaction_info, slot)?;

        // Convert Value to JSON bytes
        let payload = serde_json::to_vec(&transaction_value).map_err(|e| {
            SerializationError::SerializationFailed {
                msg: format!("Failed to convert transaction Value to JSON bytes: {e}"),
            }
        })?;

        // Create and send NATS message
        let message = NatsMessage {
            subject: self.subject.clone(),
            payload,
        };

        self.connection_manager.send_message(message)?;

        info!(
            "Successfully queued transaction {} for NATS publish",
            transaction_info.signature
        );
        Ok(())
    }

    /// Serialize and send V1 transaction
    fn serialize_and_send_v1(
        &self,
        transaction_info: &ReplicaTransactionInfo,
        slot: u64,
    ) -> Result<(), ProcessingError> {
        // Serialize transaction
        let transaction_value =
            TransactionSerializer::serialize_transaction_v1(transaction_info, slot)?;

        // Convert Value to JSON bytes
        let payload = serde_json::to_vec(&transaction_value).map_err(|e| {
            SerializationError::SerializationFailed {
                msg: format!("Failed to convert transaction Value to JSON bytes: {e}"),
            }
        })?;

        // Create and send NATS message
        let message = NatsMessage {
            subject: self.subject.clone(),
            payload,
        };

        self.connection_manager.send_message(message)?;

        info!(
            "Successfully queued transaction {} for NATS publish",
            transaction_info.signature
        );
        Ok(())
    }

    /// Determine if a transaction should be processed based on filtering rules
    fn should_process_transaction<'a>(
        &self,
        is_vote: bool,
        account_keys: impl Iterator<Item = &'a solana_sdk::pubkey::Pubkey>,
    ) -> bool {
        // Check if transaction should be processed at all
        if is_vote {
            debug!("Vote transaction detected");
        } else {
            debug!("Non-vote transaction detected");
        }

        // Apply transaction selector rules
        let selected = self
            .transaction_selector
            .is_transaction_selected(is_vote, Box::new(account_keys));

        debug!("Transaction selector result: {selected}");
        selected
    }

    /// Check if the processor is configured to handle any transactions
    pub fn is_enabled(&self) -> bool {
        self.transaction_selector.is_enabled()
    }

    /// Get a reference to the transaction selector
    pub fn transaction_selector(&self) -> &TransactionSelector {
        &self.transaction_selector
    }
}
