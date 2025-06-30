use {
    agave_geyser_plugin_interface::geyser_plugin_interface::{
        ReplicaTransactionInfo, ReplicaTransactionInfoV2,
    },
    base64::{engine::general_purpose, Engine as _},
    serde_json::Value,
    solana_geyser_plugin_nats::serializer::TransactionSerializer,
    solana_sdk::{
        instruction::{AccountMeta, Instruction},
        message::{Message, VersionedMessage},
        pubkey::Pubkey,
        signature::Signature,
        system_instruction,
        transaction::{SanitizedTransaction, VersionedTransaction},
    },
    solana_transaction_status::TransactionStatusMeta,
    std::collections::HashSet,
};

/// Helper function to create a simple test transaction
fn create_test_transaction() -> SanitizedTransaction {
    let from_pubkey = Pubkey::new_unique();
    let to_pubkey = Pubkey::new_unique();
    let lamports = 1_000_000;

    let instruction = system_instruction::transfer(&from_pubkey, &to_pubkey, lamports);
    let message = Message::new(&[instruction], Some(&from_pubkey));
    let versioned_message = VersionedMessage::Legacy(message);

    let versioned_tx = VersionedTransaction {
        message: versioned_message,
        signatures: vec![Signature::new_unique()],
    };

    let reserved_account_keys = HashSet::new();
    SanitizedTransaction::try_from_legacy_transaction(
        versioned_tx.into_legacy_transaction().unwrap(),
        &reserved_account_keys,
    )
    .expect("Failed to create sanitized transaction")
}

/// Helper function to create a test transaction with multiple instructions
fn create_complex_test_transaction() -> SanitizedTransaction {
    let from_pubkey = Pubkey::new_unique();
    let to_pubkey1 = Pubkey::new_unique();
    let to_pubkey2 = Pubkey::new_unique();
    let program_id = Pubkey::new_unique();

    let instructions = vec![
        system_instruction::transfer(&from_pubkey, &to_pubkey1, 500_000),
        system_instruction::transfer(&from_pubkey, &to_pubkey2, 300_000),
        Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(from_pubkey, true),
                AccountMeta::new_readonly(to_pubkey1, false),
            ],
            data: vec![1, 2, 3, 4, 5],
        },
    ];

    let message = Message::new(&instructions, Some(&from_pubkey));
    let versioned_message = VersionedMessage::Legacy(message);

    let versioned_tx = VersionedTransaction {
        message: versioned_message,
        signatures: vec![Signature::new_unique(), Signature::new_unique()],
    };

    let reserved_account_keys = HashSet::new();
    SanitizedTransaction::try_from_legacy_transaction(
        versioned_tx.into_legacy_transaction().unwrap(),
        &reserved_account_keys,
    )
    .expect("Failed to create complex sanitized transaction")
}

/// Helper function to create test transaction metadata
fn create_test_meta() -> TransactionStatusMeta {
    TransactionStatusMeta {
        status: Ok(()),
        fee: 5000,
        pre_balances: vec![1_000_000, 0, 1],
        post_balances: vec![994_000, 1_000_000, 1],
        log_messages: Some(vec![
            "Program 11111111111111111111111111111111 invoke [1]".to_string(),
            "Program 11111111111111111111111111111111 success".to_string(),
        ]),
        compute_units_consumed: Some(150),
        ..Default::default()
    }
}

/// Helper function to create test transaction metadata with error
fn create_error_meta() -> TransactionStatusMeta {
    TransactionStatusMeta {
        status: Err(solana_sdk::transaction::TransactionError::InsufficientFundsForFee),
        fee: 5000,
        pre_balances: vec![4000, 0, 1],
        post_balances: vec![4000, 0, 1], // No change due to error
        log_messages: Some(vec![
            "Transaction failed: InsufficientFundsForFee".to_string()
        ]),
        compute_units_consumed: Some(0),
        ..Default::default()
    }
}

#[test]
fn test_serialize_transaction_v2_success() {
    let transaction = create_test_transaction();
    let meta = create_test_meta();
    let slot = 12345;

    let transaction_info = ReplicaTransactionInfoV2 {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &meta,
        index: 0,
    };

    let result = TransactionSerializer::serialize_transaction_v2(&transaction_info, slot);
    assert!(result.is_ok());

    let serialized = result.unwrap();

    // Verify structure
    assert!(serialized.get("transaction").is_some());
    assert!(serialized.get("version").is_some());
    assert!(serialized.get("slot").is_some());
    assert!(serialized.get("meta").is_some());

    // Verify slot
    assert_eq!(serialized["slot"], slot);

    // Verify version
    assert_eq!(serialized["version"], 0);

    // Verify transaction structure
    let tx_obj = &serialized["transaction"];
    assert!(tx_obj.get("signatures").is_some());
    assert!(tx_obj.get("message").is_some());

    // Verify signatures
    let signatures = tx_obj["signatures"].as_array().unwrap();
    assert_eq!(signatures.len(), 1);
    assert_eq!(
        signatures[0].as_str().unwrap(),
        transaction.signatures()[0].to_string()
    );

    // Verify message structure
    let message = &tx_obj["message"];
    assert!(message.get("accountKeys").is_some());
    assert!(message.get("header").is_some());
    assert!(message.get("instructions").is_some());
    assert!(message.get("recentBlockhash").is_some());
    assert!(message.get("addressTableLookups").is_some());

    // Verify addressTableLookups is an empty array for V0 format
    assert_eq!(message["addressTableLookups"].as_array().unwrap().len(), 0);

    // Verify metadata
    let meta_obj = &serialized["meta"];
    assert!(meta_obj.get("err").is_some());
    assert!(meta_obj.get("fee").is_some());
    assert!(meta_obj.get("preBalances").is_some());
    assert!(meta_obj.get("postBalances").is_some());
    assert!(meta_obj.get("logMessages").is_some());
    assert!(meta_obj.get("computeUnitsConsumed").is_some());

    // Verify metadata values
    assert_eq!(meta_obj["err"], Value::Null); // Success transaction
    assert_eq!(meta_obj["fee"], 5000);
    assert_eq!(meta_obj["computeUnitsConsumed"], 150);
}

#[test]
fn test_serialize_transaction_v1_success() {
    let transaction = create_test_transaction();
    let meta = create_test_meta();
    let slot = 67890;

    let transaction_info = ReplicaTransactionInfo {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &meta,
    };

    let result = TransactionSerializer::serialize_transaction_v1(&transaction_info, slot);
    assert!(result.is_ok());

    let serialized = result.unwrap();

    // Verify structure (same as V2)
    assert!(serialized.get("transaction").is_some());
    assert!(serialized.get("version").is_some());
    assert!(serialized.get("slot").is_some());
    assert!(serialized.get("meta").is_some());

    // Verify slot
    assert_eq!(serialized["slot"], slot);

    // Verify version
    assert_eq!(serialized["version"], 0);
}

#[test]
fn test_serialize_complex_transaction_v2() {
    let transaction = create_complex_test_transaction();
    let meta = create_test_meta();
    let slot = 54321;

    let transaction_info = ReplicaTransactionInfoV2 {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &meta,
        index: 0,
    };

    let result = TransactionSerializer::serialize_transaction_v2(&transaction_info, slot);
    assert!(result.is_ok());

    let serialized = result.unwrap();

    // Verify signatures (should have 2)
    let signatures = serialized["transaction"]["signatures"].as_array().unwrap();
    assert_eq!(signatures.len(), 2);

    // Verify instructions (should have 3)
    let instructions = serialized["transaction"]["message"]["instructions"]
        .as_array()
        .unwrap();
    assert_eq!(instructions.len(), 3);

    // Verify instruction structure
    let instruction = &instructions[2]; // Third instruction (custom one)
    assert!(instruction.get("programIdIndex").is_some());
    assert!(instruction.get("accounts").is_some());
    assert!(instruction.get("data").is_some());

    // Verify data is base64 encoded
    let data_str = instruction["data"].as_str().unwrap();
    let decoded_data = general_purpose::STANDARD.decode(data_str).unwrap();
    assert_eq!(decoded_data, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_serialize_transaction_with_error_meta() {
    let transaction = create_test_transaction();
    let meta = create_error_meta();
    let slot = 11111;

    let transaction_info = ReplicaTransactionInfoV2 {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &meta,
        index: 0,
    };

    let result = TransactionSerializer::serialize_transaction_v2(&transaction_info, slot);
    assert!(result.is_ok());

    let serialized = result.unwrap();
    let meta_obj = &serialized["meta"];

    // Verify error is captured
    assert!(!meta_obj["err"].is_null());
    let error_str = meta_obj["err"].as_str().unwrap();
    assert!(error_str.contains("InsufficientFundsForFee"));

    // Verify compute units consumed is 0 for failed transaction
    assert_eq!(meta_obj["computeUnitsConsumed"], 0);
}

#[test]
fn test_serialize_transaction_with_default_meta() {
    let transaction = create_test_transaction();
    let slot = 99999;

    // Create transaction info with default metadata
    let default_meta = TransactionStatusMeta::default();
    let transaction_info = ReplicaTransactionInfoV2 {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &default_meta,
        index: 0,
    };

    let result = TransactionSerializer::serialize_transaction_v2(&transaction_info, slot);
    assert!(result.is_ok());

    let serialized = result.unwrap();

    // Should still have meta field but with default values
    assert!(serialized.get("meta").is_some());
    let meta_obj = &serialized["meta"];
    assert_eq!(meta_obj["fee"], 0);
    assert_eq!(meta_obj["computeUnitsConsumed"], Value::Null);
}

#[test]
fn test_serialize_transaction_v1_and_v2_consistency() {
    let transaction = create_test_transaction();
    let meta = create_test_meta();
    let slot = 12345;

    let transaction_info_v1 = ReplicaTransactionInfo {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &meta,
    };

    let transaction_info_v2 = ReplicaTransactionInfoV2 {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &meta,
        index: 0,
    };

    let result_v1 = TransactionSerializer::serialize_transaction_v1(&transaction_info_v1, slot);
    let result_v2 = TransactionSerializer::serialize_transaction_v2(&transaction_info_v2, slot);

    assert!(result_v1.is_ok());
    assert!(result_v2.is_ok());

    let serialized_v1 = result_v1.unwrap();
    let serialized_v2 = result_v2.unwrap();

    // Both should produce identical results
    assert_eq!(serialized_v1, serialized_v2);
}

#[test]
fn test_serialize_message_structure() {
    let transaction = create_test_transaction();
    let meta = create_test_meta();
    let slot = 12345;

    let transaction_info = ReplicaTransactionInfoV2 {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &meta,
        index: 0,
    };

    let result = TransactionSerializer::serialize_transaction_v2(&transaction_info, slot);
    assert!(result.is_ok());

    let serialized = result.unwrap();
    let message = &serialized["transaction"]["message"];

    // Verify header structure
    let header = &message["header"];
    assert!(header.get("numRequiredSignatures").is_some());
    assert!(header.get("numReadonlySignedAccounts").is_some());
    assert!(header.get("numReadonlyUnsignedAccounts").is_some());

    // Verify account keys are strings
    let account_keys = message["accountKeys"].as_array().unwrap();
    for account_key in account_keys {
        assert!(account_key.is_string());
        // Verify it's a valid base58 string (pubkey format)
        let key_str = account_key.as_str().unwrap();
        assert!(!key_str.is_empty());
    }

    // Verify recent blockhash is a string
    assert!(message["recentBlockhash"].is_string());
    let blockhash_str = message["recentBlockhash"].as_str().unwrap();
    assert!(!blockhash_str.is_empty());
}

#[test]
fn test_serialize_instruction_data_encoding() {
    let transaction = create_complex_test_transaction();
    let meta = create_test_meta();
    let slot = 12345;

    let transaction_info = ReplicaTransactionInfoV2 {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &meta,
        index: 0,
    };

    let result = TransactionSerializer::serialize_transaction_v2(&transaction_info, slot);
    assert!(result.is_ok());

    let serialized = result.unwrap();
    let instructions = serialized["transaction"]["message"]["instructions"]
        .as_array()
        .unwrap();

    // Check that all instructions have properly encoded data
    for instruction in instructions {
        let data_str = instruction["data"].as_str().unwrap();

        // Verify base64 decoding works
        let decode_result = general_purpose::STANDARD.decode(data_str);
        assert!(decode_result.is_ok());

        // Verify accounts is an array of numbers
        let accounts = instruction["accounts"].as_array().unwrap();
        for account in accounts {
            assert!(account.is_number());
        }

        // Verify programIdIndex is a number
        assert!(instruction["programIdIndex"].is_number());
    }
}

#[test]
fn test_serialize_balances_and_logs() {
    let transaction = create_test_transaction();
    let meta = create_test_meta();
    let slot = 12345;

    let transaction_info = ReplicaTransactionInfoV2 {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &meta,
        index: 0,
    };

    let result = TransactionSerializer::serialize_transaction_v2(&transaction_info, slot);
    assert!(result.is_ok());

    let serialized = result.unwrap();
    let meta_obj = &serialized["meta"];

    // Verify balances are arrays of numbers
    let pre_balances = meta_obj["preBalances"].as_array().unwrap();
    let post_balances = meta_obj["postBalances"].as_array().unwrap();

    assert_eq!(pre_balances.len(), 3);
    assert_eq!(post_balances.len(), 3);

    for balance in pre_balances {
        assert!(balance.is_number());
    }

    for balance in post_balances {
        assert!(balance.is_number());
    }

    // Verify log messages are strings
    let log_messages = meta_obj["logMessages"].as_array().unwrap();
    assert_eq!(log_messages.len(), 2);

    for log in log_messages {
        assert!(log.is_string());
        assert!(!log.as_str().unwrap().is_empty());
    }
}

#[test]
fn test_serialize_vote_transaction() {
    let transaction = create_test_transaction();
    let meta = create_test_meta();
    let slot = 12345;

    let transaction_info = ReplicaTransactionInfoV2 {
        signature: &transaction.signatures()[0],
        is_vote: true, // Mark as vote transaction
        transaction: &transaction,
        transaction_status_meta: &meta,
        index: 0,
    };

    let result = TransactionSerializer::serialize_transaction_v2(&transaction_info, slot);
    assert!(result.is_ok());

    let serialized = result.unwrap();

    // Vote transactions should serialize the same way as regular transactions
    // The is_vote flag is not included in the serialized output but is used
    // for filtering in the processor
    assert!(serialized.get("transaction").is_some());
    assert!(serialized.get("version").is_some());
    assert!(serialized.get("slot").is_some());
    assert!(serialized.get("meta").is_some());
}

#[test]
fn test_serialize_large_slot_number() {
    let transaction = create_test_transaction();
    let meta = create_test_meta();
    let slot = u64::MAX; // Test with maximum slot number

    let transaction_info = ReplicaTransactionInfoV2 {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &meta,
        index: 0,
    };

    let result = TransactionSerializer::serialize_transaction_v2(&transaction_info, slot);
    assert!(result.is_ok());

    let serialized = result.unwrap();
    assert_eq!(serialized["slot"], slot);
}

#[test]
fn test_serialize_empty_log_messages() {
    let transaction = create_test_transaction();
    let mut meta = create_test_meta();
    meta.log_messages = Some(vec![]); // Empty log messages
    let slot = 12345;

    let transaction_info = ReplicaTransactionInfoV2 {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &meta,
        index: 0,
    };

    let result = TransactionSerializer::serialize_transaction_v2(&transaction_info, slot);
    assert!(result.is_ok());

    let serialized = result.unwrap();
    let meta_obj = &serialized["meta"];

    // Should have empty array for log messages
    let log_messages = meta_obj["logMessages"].as_array().unwrap();
    assert_eq!(log_messages.len(), 0);
}

#[test]
fn test_serialize_no_log_messages() {
    let transaction = create_test_transaction();
    let mut meta = create_test_meta();
    meta.log_messages = None; // No log messages
    let slot = 12345;

    let transaction_info = ReplicaTransactionInfoV2 {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &meta,
        index: 0,
    };

    let result = TransactionSerializer::serialize_transaction_v2(&transaction_info, slot);
    assert!(result.is_ok());

    let serialized = result.unwrap();
    let meta_obj = &serialized["meta"];

    // Should have empty array for log messages when None
    let log_messages = meta_obj["logMessages"].as_array().unwrap();
    assert_eq!(log_messages.len(), 0);
}

#[test]
fn test_json_serialization_roundtrip() {
    let transaction = create_test_transaction();
    let meta = create_test_meta();
    let slot = 12345;

    let transaction_info = ReplicaTransactionInfoV2 {
        signature: &transaction.signatures()[0],
        is_vote: false,
        transaction: &transaction,
        transaction_status_meta: &meta,
        index: 0,
    };

    let result = TransactionSerializer::serialize_transaction_v2(&transaction_info, slot);
    assert!(result.is_ok());

    let serialized = result.unwrap();

    // Test that the serialized JSON can be converted to string and back
    let json_string = serde_json::to_string(&serialized).unwrap();
    let parsed_back: Value = serde_json::from_str(&json_string).unwrap();

    assert_eq!(serialized, parsed_back);
}

#[test]
fn test_serialize_multiple_transactions_consistency() {
    // Test that serializing multiple transactions produces consistent results
    let transactions = vec![create_test_transaction(), create_complex_test_transaction()];

    let meta = create_test_meta();
    let slot = 12345;

    for (i, transaction) in transactions.iter().enumerate() {
        let transaction_info = ReplicaTransactionInfoV2 {
            signature: &transaction.signatures()[0],
            is_vote: false,
            transaction,
            transaction_status_meta: &meta,
            index: i,
        };

        let result =
            TransactionSerializer::serialize_transaction_v2(&transaction_info, slot + i as u64);
        assert!(result.is_ok(), "Failed to serialize transaction {i}");

        let serialized = result.unwrap();

        // Verify all transactions have the same structure
        assert!(serialized.get("transaction").is_some());
        assert!(serialized.get("version").is_some());
        assert!(serialized.get("slot").is_some());
        assert!(serialized.get("meta").is_some());

        // Verify slot is correct
        assert_eq!(serialized["slot"], slot + i as u64);
    }
}
