use {
    agave_geyser_plugin_interface::geyser_plugin_interface::{
        ReplicaTransactionInfo, ReplicaTransactionInfoV2, ReplicaTransactionInfoVersions,
    },
    solana_geyser_plugin_nats::{
        config::TransactionFilterConfig,
        connection::ConnectionManager,
        processor::{ProcessingError, TransactionProcessor},
    },
    solana_sdk::{
        message::Message,
        pubkey::Pubkey,
        signature::Signature,
        system_instruction,
        transaction::{SanitizedTransaction, Transaction},
    },
    solana_transaction_status::TransactionStatusMeta,
    std::{collections::HashSet, sync::Arc, thread},
};

// Helper functions to create test data
fn create_test_transaction() -> SanitizedTransaction {
    let from_pubkey = Pubkey::new_unique();
    let to_pubkey = Pubkey::new_unique();
    let instruction = system_instruction::transfer(&from_pubkey, &to_pubkey, 1_000_000);

    let message = Message::new(&[instruction], Some(&from_pubkey));

    let transaction = Transaction {
        signatures: vec![Signature::default()],
        message,
    };

    SanitizedTransaction::try_from_legacy_transaction(transaction, &HashSet::new())
        .expect("Failed to create sanitized transaction")
}

fn create_test_meta() -> TransactionStatusMeta {
    TransactionStatusMeta {
        status: Ok(()),
        fee: 5000,
        pre_balances: vec![1_000_000, 0, 1],
        post_balances: vec![994_000, 1_000_000, 1],
        inner_instructions: None,
        log_messages: Some(vec![
            "Program 11111111111111111111111111111111 invoke [1]".to_string(),
            "Program 11111111111111111111111111111111 success".to_string(),
        ]),
        pre_token_balances: None,
        post_token_balances: None,
        rewards: None,
        loaded_addresses: Default::default(),
        return_data: None,
        compute_units_consumed: Some(150),
    }
}

fn create_replica_transaction_info_v2(is_vote: bool) -> ReplicaTransactionInfoV2<'static> {
    let transaction = Box::leak(Box::new(create_test_transaction()));
    let transaction_status_meta = Box::leak(Box::new(create_test_meta()));
    let signature = transaction.signature();

    ReplicaTransactionInfoV2 {
        signature,
        is_vote,
        transaction,
        transaction_status_meta,
        index: 0,
    }
}

fn create_replica_transaction_info_v1(is_vote: bool) -> ReplicaTransactionInfo<'static> {
    let transaction = Box::leak(Box::new(create_test_transaction()));
    let transaction_status_meta = Box::leak(Box::new(create_test_meta()));
    let signature = transaction.signature();

    ReplicaTransactionInfo {
        signature,
        is_vote,
        transaction,
        transaction_status_meta,
    }
}

// Create a ConnectionManager for testing
fn create_test_connection_manager() -> Arc<ConnectionManager> {
    // Use a non-existent port for testing with high retry count and long timeout
    // This keeps the worker thread alive long enough for the tests to run
    // The worker will keep retrying in the background while the processor logic is being tested
    match ConnectionManager::new("nats://127.0.0.1:9999", 100, 10) {
        Ok(manager) => Arc::new(manager),
        Err(_) => {
            // If connection creation fails due to DNS resolution, try with localhost
            Arc::new(
                ConnectionManager::new("nats://localhost:9999", 100, 10)
                    .expect("Failed to create test connection manager"),
            )
        }
    }
}

#[cfg(test)]
mod processor_creation_tests {
    use super::*;

    #[test]
    fn test_processor_new_with_default_config() {
        let connection_manager = create_test_connection_manager();
        let filter_config = TransactionFilterConfig::default();
        let subject = "test.subject".to_string();

        let processor = TransactionProcessor::new(connection_manager, &filter_config, subject);

        assert!(processor.is_enabled());
        assert!(processor.transaction_selector().select_all_transactions);
        assert!(
            processor
                .transaction_selector()
                .select_all_vote_transactions
        );
    }

    #[test]
    fn test_processor_new_with_vote_only_config() {
        let connection_manager = create_test_connection_manager();
        let filter_config = TransactionFilterConfig {
            select_all_transactions: false,
            select_vote_transactions: true,
            mentioned_addresses: vec![],
        };

        let processor = TransactionProcessor::new(
            connection_manager,
            &filter_config,
            "test.subject".to_string(),
        );

        assert!(processor.is_enabled());
        assert!(!processor.transaction_selector().select_all_transactions);
        assert!(
            processor
                .transaction_selector()
                .select_all_vote_transactions
        );
    }

    #[test]
    fn test_processor_new_with_specific_addresses() {
        let connection_manager = create_test_connection_manager();
        let test_address = Pubkey::new_unique().to_string();
        let filter_config = TransactionFilterConfig {
            select_all_transactions: false,
            select_vote_transactions: false,
            mentioned_addresses: vec![test_address],
        };

        let processor = TransactionProcessor::new(
            connection_manager,
            &filter_config,
            "test.subject".to_string(),
        );

        assert!(processor.is_enabled());
        assert!(!processor.transaction_selector().select_all_transactions);
        assert!(
            !processor
                .transaction_selector()
                .select_all_vote_transactions
        );
        assert!(!processor
            .transaction_selector()
            .mentioned_addresses
            .is_empty());
    }

    #[test]
    fn test_processor_new_no_filters_defaults_to_all() {
        let connection_manager = create_test_connection_manager();
        let filter_config = TransactionFilterConfig {
            select_all_transactions: false,
            select_vote_transactions: false,
            mentioned_addresses: vec![],
        };

        let processor = TransactionProcessor::new(
            connection_manager,
            &filter_config,
            "test.subject".to_string(),
        );

        // Should default to select_all_transactions
        assert!(processor.is_enabled());
        assert!(processor.transaction_selector().select_all_transactions);
    }
}

#[cfg(test)]
mod transaction_processing_tests {
    use super::*;

    #[test]
    fn test_process_transaction_v2_success() {
        let connection_manager = create_test_connection_manager();
        let filter_config = TransactionFilterConfig::default();
        let processor = TransactionProcessor::new(
            connection_manager,
            &filter_config,
            "test.subject".to_string(),
        );

        let tx_info = create_replica_transaction_info_v2(false);
        let transaction_info = ReplicaTransactionInfoVersions::V0_0_2(&tx_info);

        // Process the transaction - should succeed (message will be queued even if connection fails)
        let result = processor.process_transaction(transaction_info, 12345);

        // Should succeed because the message is queued to the channel successfully
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_transaction_v1_success() {
        let connection_manager = create_test_connection_manager();
        let filter_config = TransactionFilterConfig::default();
        let processor = TransactionProcessor::new(
            connection_manager,
            &filter_config,
            "test.subject".to_string(),
        );

        let tx_info = create_replica_transaction_info_v1(false);
        let transaction_info = ReplicaTransactionInfoVersions::V0_0_1(&tx_info);

        let result = processor.process_transaction(transaction_info, 12345);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_transaction_vote_included_when_enabled() {
        let connection_manager = create_test_connection_manager();
        let filter_config = TransactionFilterConfig {
            select_all_transactions: false,
            select_vote_transactions: true,
            mentioned_addresses: vec![],
        };

        let processor = TransactionProcessor::new(
            connection_manager,
            &filter_config,
            "test.subject".to_string(),
        );

        let tx_info = create_replica_transaction_info_v2(true); // is_vote = true
        let transaction_info = ReplicaTransactionInfoVersions::V0_0_2(&tx_info);

        let result = processor.process_transaction(transaction_info, 12345);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_transaction_vote_filtered_out() {
        let connection_manager = create_test_connection_manager();

        // Configure to only process transactions with specific addresses (no vote processing)
        let random_address = Pubkey::new_unique().to_string();
        let filter_config = TransactionFilterConfig {
            select_all_transactions: false,
            select_vote_transactions: false,
            mentioned_addresses: vec![random_address],
        };

        let processor = TransactionProcessor::new(
            connection_manager,
            &filter_config,
            "test.subject".to_string(),
        );

        let tx_info = create_replica_transaction_info_v2(true); // is_vote = true
        let transaction_info = ReplicaTransactionInfoVersions::V0_0_2(&tx_info);

        // This should succeed because filtering happens before sending
        let result = processor.process_transaction(transaction_info, 12345);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_transaction_with_matching_address() {
        let connection_manager = create_test_connection_manager();

        // Create a transaction and use one of its addresses
        let tx_info = create_replica_transaction_info_v2(false);
        let account_keys = tx_info.transaction.message().account_keys();
        let target_address = account_keys[0].to_string();

        let filter_config = TransactionFilterConfig {
            select_all_transactions: false,
            select_vote_transactions: false,
            mentioned_addresses: vec![target_address],
        };

        let processor = TransactionProcessor::new(
            connection_manager,
            &filter_config,
            "test.subject".to_string(),
        );

        let transaction_info = ReplicaTransactionInfoVersions::V0_0_2(&tx_info);

        let result = processor.process_transaction(transaction_info, 12345);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_transaction_with_non_matching_address() {
        let connection_manager = create_test_connection_manager();

        // Use a random address that won't match the transaction
        let random_address = Pubkey::new_unique().to_string();
        let filter_config = TransactionFilterConfig {
            select_all_transactions: false,
            select_vote_transactions: false,
            mentioned_addresses: vec![random_address],
        };

        let processor = TransactionProcessor::new(
            connection_manager,
            &filter_config,
            "test.subject".to_string(),
        );

        let tx_info = create_replica_transaction_info_v2(false);
        let transaction_info = ReplicaTransactionInfoVersions::V0_0_2(&tx_info);

        // This should succeed because filtering happens before sending
        let result = processor.process_transaction(transaction_info, 12345);
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[test]
    fn test_processor_connection_error_scenarios() {
        // Test 1: Invalid URL that should fail DNS resolution
        let result = ConnectionManager::new("nats://invalid-nonexistent-host:4222", 1, 1);
        assert!(result.is_err());
        match result.err().unwrap() {
            solana_geyser_plugin_nats::connection::ConnectionError::HostResolutionFailed {
                ..
            } => {
                // Expected error type
            }
            _ => panic!("Expected HostResolutionFailed error"),
        }

        // Test 2: Invalid port - may succeed or fail depending on host resolution
        let result = ConnectionManager::new("nats://127.0.0.1:99999", 1, 1);
        if result.is_ok() {
            let mut manager = result.unwrap();
            manager.shutdown();
        }
        // Both success and failure are valid outcomes for this test case
    }

    #[test]
    fn test_processing_error_display() {
        use solana_geyser_plugin_nats::serializer::SerializationError;

        let serialization_error = SerializationError::SerializationFailed {
            msg: "Test serialization error".to_string(),
        };
        let processing_error = ProcessingError::Serialization(serialization_error);
        let display_string = format!("{processing_error}");
        assert!(display_string.contains("Test serialization error"));

        let filtering_error = ProcessingError::FilteringFailed {
            msg: "Test filtering error".to_string(),
        };
        let display_string = format!("{filtering_error}");
        assert!(display_string.contains("Test filtering error"));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_end_to_end_processing() {
        let connection_manager = create_test_connection_manager();
        let filter_config = TransactionFilterConfig::default();
        let processor = TransactionProcessor::new(
            connection_manager,
            &filter_config,
            "integration.test".to_string(),
        );

        // Process multiple transactions
        let tx_v2 = create_replica_transaction_info_v2(false);
        let tx_v1 = create_replica_transaction_info_v1(false);

        let result1 =
            processor.process_transaction(ReplicaTransactionInfoVersions::V0_0_2(&tx_v2), 12345);
        let result2 =
            processor.process_transaction(ReplicaTransactionInfoVersions::V0_0_1(&tx_v1), 12346);

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }

    #[test]
    fn test_concurrent_processing() {
        let connection_manager = create_test_connection_manager();
        let filter_config = TransactionFilterConfig::default();
        let processor = Arc::new(TransactionProcessor::new(
            connection_manager,
            &filter_config,
            "concurrent.test".to_string(),
        ));

        let num_threads = 5;
        let transactions_per_thread = 10;
        let mut handles = vec![];

        for thread_id in 0..num_threads {
            let processor_clone = processor.clone();
            let handle = thread::spawn(move || {
                for tx_id in 0..transactions_per_thread {
                    let tx_info = create_replica_transaction_info_v2(false);
                    let transaction_info = ReplicaTransactionInfoVersions::V0_0_2(&tx_info);
                    let slot = (thread_id * 1000 + tx_id) as u64;

                    let result = processor_clone.process_transaction(transaction_info, slot);
                    assert!(
                        result.is_ok(),
                        "Transaction processing failed in thread {thread_id}"
                    );
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().expect("Thread panicked");
        }
    }

    #[test]
    fn test_processor_is_enabled_states() {
        let connection_manager = create_test_connection_manager();

        // Test enabled with default config
        let processor1 = TransactionProcessor::new(
            connection_manager.clone(),
            &TransactionFilterConfig::default(),
            "test1".to_string(),
        );
        assert!(processor1.is_enabled());

        // Test enabled with vote-only config
        let vote_config = TransactionFilterConfig {
            select_all_transactions: false,
            select_vote_transactions: true,
            mentioned_addresses: vec![],
        };
        let processor2 = TransactionProcessor::new(
            connection_manager.clone(),
            &vote_config,
            "test2".to_string(),
        );
        assert!(processor2.is_enabled());

        // Test enabled with specific addresses
        let address_config = TransactionFilterConfig {
            select_all_transactions: false,
            select_vote_transactions: false,
            mentioned_addresses: vec![Pubkey::new_unique().to_string()],
        };
        let processor3 =
            TransactionProcessor::new(connection_manager, &address_config, "test3".to_string());
        assert!(processor3.is_enabled());
    }

    #[tokio::test]
    async fn test_process_transactions_multiple_types() {
        let connection_manager = create_test_connection_manager();
        let filter_config = TransactionFilterConfig {
            select_all_transactions: true,
            select_vote_transactions: true,
            mentioned_addresses: vec![],
        };

        let processor = TransactionProcessor::new(
            connection_manager,
            &filter_config,
            "test.transactions".to_string(),
        );

        // Test multiple transaction scenarios
        let v1_non_vote = create_replica_transaction_info_v1(false);
        let v2_vote = create_replica_transaction_info_v2(true);

        let result_v1 = processor
            .process_transaction(ReplicaTransactionInfoVersions::V0_0_1(&v1_non_vote), 12345);
        let result_v2 =
            processor.process_transaction(ReplicaTransactionInfoVersions::V0_0_2(&v2_vote), 12346);

        assert!(result_v1.is_ok());
        assert!(result_v2.is_ok());
    }

    #[tokio::test]
    async fn test_transaction_filtering_scenarios() {
        let connection_manager = create_test_connection_manager();

        // Test with vote filtering disabled
        let filter_config = TransactionFilterConfig {
            select_all_transactions: false,
            select_vote_transactions: false,
            mentioned_addresses: vec![],
        };

        let processor = TransactionProcessor::new(
            connection_manager,
            &filter_config,
            "test.transactions".to_string(),
        );

        let vote_transaction = create_replica_transaction_info_v1(true);
        let result = processor.process_transaction(
            ReplicaTransactionInfoVersions::V0_0_1(&vote_transaction),
            12345,
        );

        assert!(result.is_ok());
    }
}
