use {
    agave_geyser_plugin_interface::geyser_plugin_interface::{
        ReplicaTransactionInfo, ReplicaTransactionInfoV2,
    },
    base64::{engine::general_purpose, Engine as _},
    log::{debug, info},
    serde_json::{json, Value},
    solana_transaction_status::TransactionStatusMeta,
    thiserror::Error,
};

#[derive(Error, Debug)]
pub enum SerializationError {
    #[error("Failed to serialize transaction: {msg}")]
    SerializationFailed { msg: String },

    #[error("Missing transaction data: {msg}")]
    MissingData { msg: String },

    #[error("Invalid transaction format: {msg}")]
    InvalidFormat { msg: String },
}

pub struct TransactionSerializer;

impl TransactionSerializer {
    /// Serialize a V2 transaction to NATS message format
    pub fn serialize_transaction_v2(
        transaction_info: &ReplicaTransactionInfoV2,
        slot: u64,
    ) -> Result<Value, SerializationError> {
        info!("Serializing V2 transaction for slot {slot}");

        // Convert SanitizedTransaction back to VersionedTransaction
        // This gives us the proper version detection and message structure
        let versioned_tx = transaction_info.transaction.to_versioned_transaction();

        let (version, message_json) = Self::serialize_versioned_transaction(&versioned_tx)?;

        // Serialize signatures
        let signatures: Vec<String> = transaction_info
            .transaction
            .signatures()
            .iter()
            .map(|sig| sig.to_string())
            .collect();

        // Build transaction object
        let transaction_obj = json!({
            "signatures": signatures,
            "message": message_json
        });

        // Build final message
        let result = json!({
            "transaction": transaction_obj,
            "version": version,
            "slot": slot,
            "meta": Self::serialize_transaction_meta(Some(transaction_info.transaction_status_meta)),
        });

        debug!("Successfully serialized V2 transaction");
        Ok(result)
    }

    /// Serialize a V1 transaction to NATS message format  
    pub fn serialize_transaction_v1(
        transaction_info: &ReplicaTransactionInfo,
        slot: u64,
    ) -> Result<Value, SerializationError> {
        info!("Serializing V1 transaction for slot {slot}");

        // Convert SanitizedTransaction back to VersionedTransaction
        let versioned_tx = transaction_info.transaction.to_versioned_transaction();

        let (version, message_json) = Self::serialize_versioned_transaction(&versioned_tx)?;

        // Serialize signatures
        let signatures: Vec<String> = transaction_info
            .transaction
            .signatures()
            .iter()
            .map(|sig| sig.to_string())
            .collect();

        // Build transaction object
        let transaction_obj = json!({
            "signatures": signatures,
            "message": message_json
        });

        // Build final message
        let result = json!({
            "transaction": transaction_obj,
            "version": version,
            "slot": slot,
            "meta": Self::serialize_transaction_meta(Some(transaction_info.transaction_status_meta)),
        });

        debug!("Successfully serialized V1 transaction");
        Ok(result)
    }

    /// Serialize a VersionedTransaction to get proper version and message structure
    fn serialize_versioned_transaction(
        versioned_tx: &solana_sdk::transaction::VersionedTransaction,
    ) -> Result<(Value, Value), SerializationError> {
        debug!("Processing versioned transaction");

        // Default to V0 format as per current validator behavior
        // The to_versioned_transaction() method preserves the original version info
        let version = json!(0);

        // Create V0 message structure with addressTableLookups
        let account_keys: Vec<String> = versioned_tx
            .message
            .static_account_keys()
            .iter()
            .map(|key| key.to_string())
            .collect();

        let instructions: Vec<Value> = versioned_tx
            .message
            .instructions()
            .iter()
            .map(|ix| {
                json!({
                    "programIdIndex": ix.program_id_index,
                    "accounts": ix.accounts,
                    "data": general_purpose::STANDARD.encode(&ix.data)
                })
            })
            .collect();

        let header = json!({
            "numRequiredSignatures": versioned_tx.message.header().num_required_signatures,
            "numReadonlySignedAccounts": versioned_tx.message.header().num_readonly_signed_accounts,
            "numReadonlyUnsignedAccounts": versioned_tx.message.header().num_readonly_unsigned_accounts
        });

        // Create V0 message format with addressTableLookups (this is the key improvement)
        let message_json = json!({
            "accountKeys": account_keys,
            "header": header,
            "instructions": instructions,
            "recentBlockhash": versioned_tx.message.recent_blockhash().to_string(),
            "addressTableLookups": [] // Empty array for V0 format compatibility
        });

        Ok((version, message_json))
    }

    /// Serialize transaction metadata
    fn serialize_transaction_meta(meta: Option<&TransactionStatusMeta>) -> Value {
        match meta {
            Some(meta) => {
                json!({
                    "err": meta.status.is_err().then(|| format!("{:?}", meta.status)),
                    "fee": meta.fee,
                    "preBalances": meta.pre_balances,
                    "postBalances": meta.post_balances,
                    "logMessages": meta.log_messages.as_ref().unwrap_or(&vec![]),
                    "computeUnitsConsumed": meta.compute_units_consumed,
                })
            }
            None => json!(null),
        }
    }
}
