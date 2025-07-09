use agave_geyser_plugin_interface::geyser_plugin_interface::{
    GeyserPlugin, GeyserPluginError, ReplicaAccountInfoVersions, SlotStatus,
};
use solana_geyser_plugin_nats::{GeyserPluginNats, NatsPluginConfig, TransactionFilterConfig};
use std::fs;
use tempfile::NamedTempFile;

mod test_helpers;
use test_helpers::{NatsServerError, NatsTestServer};

#[test]
fn test_plugin_creation_and_properties() {
    let plugin = GeyserPluginNats::new();

    // Test basic properties
    assert_eq!(plugin.name(), "GeyserPluginNats");
    assert!(!plugin.account_data_notifications_enabled());
    assert!(!plugin.transaction_notifications_enabled());
}

#[test]
fn test_config_loading_with_nats_server() {
    // Try to start NATS server, skip test if not available
    let nats_server = match NatsTestServer::start() {
        Ok(server) => server,
        Err(NatsServerError::BinaryNotFound) => {
            println!("Skipping test: nats-server binary not found. Install nats-server to run this test.");
            return;
        }
        Err(e) => panic!("Failed to start NATS server: {e}"),
    };

    let nats_url = format!("nats://{}", nats_server.url());
    let subject = "solana.transactions.test";

    // Create and configure plugin
    let mut plugin = GeyserPluginNats::new();

    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let config = NatsPluginConfig {
        nats_url: nats_url.clone(),
        subject: subject.to_string(),
        max_retries: 5,
        timeout_secs: 10,
        filter: TransactionFilterConfig::default(),
    };
    let config_json = serde_json::to_string(&config).expect("Failed to serialize config");
    fs::write(&temp_file, config_json).expect("Failed to write to temp file");

    // Load plugin - should succeed with real NATS server
    plugin
        .on_load(temp_file.path().to_str().unwrap(), false)
        .expect("Plugin should load successfully with NATS server running");

    // Verify plugin is enabled for transactions
    assert!(plugin.transaction_notifications_enabled());

    // Test that ignored operations still work
    let result = plugin.notify_end_of_startup();
    assert!(result.is_ok());

    // Clean up
    plugin.on_unload();
}

#[test]
fn test_config_error_scenarios() {
    let mut plugin = GeyserPluginNats::new();

    // Test 1: File not found
    let result = plugin.on_load("nonexistent_config.json", false);
    assert!(result.is_err());
    if let Err(GeyserPluginError::ConfigFileReadError { .. }) = result {
        // Expected error type
    } else {
        panic!("Expected ConfigFileReadError for missing file");
    }

    // Test 2: Invalid JSON
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    fs::write(&temp_file, "invalid json content").expect("Failed to write to temp file");

    let result = plugin.on_load(temp_file.path().to_str().unwrap(), false);
    assert!(result.is_err());
    if let Err(GeyserPluginError::ConfigFileReadError { .. }) = result {
        // Expected error type
    } else {
        panic!("Expected ConfigFileReadError for invalid JSON");
    }

    // Test 3: Missing required fields
    let temp_file2 = NamedTempFile::new().expect("Failed to create temp file");
    fs::write(&temp_file2, r#"{"some_field": "some_value"}"#)
        .expect("Failed to write to temp file");

    let result = plugin.on_load(temp_file2.path().to_str().unwrap(), false);
    assert!(result.is_err());
}

#[test]
fn test_plugin_unload() {
    let mut plugin = GeyserPluginNats::new();

    // Should not panic when unloading an uninitialized plugin
    plugin.on_unload();
}

#[test]
fn test_update_account_ignored() {
    let plugin = GeyserPluginNats::new();

    // Account updates should be ignored and return Ok
    let result = plugin.update_account(
        ReplicaAccountInfoVersions::V0_0_1(
            &agave_geyser_plugin_interface::geyser_plugin_interface::ReplicaAccountInfo {
                pubkey: &[0u8; 32],
                lamports: 1000,
                owner: &[0u8; 32],
                executable: false,
                rent_epoch: 0,
                data: &[],
                write_version: 1,
            },
        ),
        12345,
        false,
    );

    assert!(result.is_ok());
}

#[test]
fn test_update_slot_status_ignored() {
    let plugin = GeyserPluginNats::new();

    // Slot status updates should be ignored and return Ok
    let result = plugin.update_slot_status(12345, Some(12344), &SlotStatus::Processed);
    assert!(result.is_ok());

    let result = plugin.update_slot_status(12346, Some(12345), &SlotStatus::Confirmed);
    assert!(result.is_ok());

    let result = plugin.update_slot_status(12347, Some(12346), &SlotStatus::Rooted);
    assert!(result.is_ok());
}

#[test]
fn test_notify_end_of_startup() {
    let plugin = GeyserPluginNats::new();

    // Should return Ok without doing anything
    let result = plugin.notify_end_of_startup();
    assert!(result.is_ok());
}

#[test]
fn test_notify_block_metadata_ignored() {
    let plugin = GeyserPluginNats::new();

    // Block metadata should be ignored and return Ok
    let result = plugin.notify_block_metadata(
        agave_geyser_plugin_interface::geyser_plugin_interface::ReplicaBlockInfoVersions::V0_0_1(
            &agave_geyser_plugin_interface::geyser_plugin_interface::ReplicaBlockInfo {
                slot: 12345,
                blockhash: "test_blockhash",
                rewards: &[],
                block_time: None,
                block_height: Some(12345),
            },
        ),
    );

    assert!(result.is_ok());
}

#[test]
fn test_c_plugin_interface() {
    // Test the C interface function
    unsafe {
        let plugin_ptr = solana_geyser_plugin_nats::_create_plugin();
        assert!(!plugin_ptr.is_null());

        // Get the plugin reference
        let plugin = &*plugin_ptr;
        assert_eq!(plugin.name(), "GeyserPluginNats");

        // Clean up
        let _ = Box::from_raw(plugin_ptr);
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let config = NatsPluginConfig {
            nats_url: "nats://localhost:4222".to_string(),
            subject: "solana.transactions".to_string(),
            max_retries: 5,
            timeout_secs: 10,
            filter: TransactionFilterConfig::default(),
        };

        let json = serde_json::to_string(&config).expect("Failed to serialize");
        let deserialized: NatsPluginConfig =
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(config.nats_url, deserialized.nats_url);
        assert_eq!(config.subject, deserialized.subject);
    }

    #[test]
    fn test_config_with_custom_subject() {
        let config = NatsPluginConfig {
            nats_url: "nats://custom.host:9999".to_string(),
            subject: "custom.subject.transactions".to_string(),
            max_retries: 5,
            timeout_secs: 10,
            filter: TransactionFilterConfig::default(),
        };

        let json = serde_json::to_string(&config).expect("Failed to serialize");
        assert!(json.contains("custom.host"));
        assert!(json.contains("custom.subject.transactions"));
    }
}
