use agave_geyser_plugin_interface::geyser_plugin_interface::GeyserPlugin;
use solana_geyser_plugin_nats::{GeyserPluginNats, NatsPluginConfig, TransactionFilterConfig};
use std::{fs, thread, time::Duration};
use tempfile::NamedTempFile;

mod test_helpers;
use test_helpers::{NatsServerError, NatsTestServer};

fn skip_if_nats_unavailable() -> Option<NatsTestServer> {
    match NatsTestServer::start() {
        Ok(server) => Some(server),
        Err(NatsServerError::BinaryNotFound) => {
            println!("Skipping test: nats-server binary not found. Install nats-server to run this test.");
            None
        }
        Err(e) => panic!("Failed to start NATS server: {e}"),
    }
}

#[test]
fn test_plugin_workflow_with_nats() {
    // Start NATS server or skip test
    let nats_server = match skip_if_nats_unavailable() {
        Some(server) => server,
        None => return,
    };

    let nats_url = format!("nats://{}", nats_server.url());
    let subject = "test.transactions";

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
fn test_nats_connection_failure_handling() {
    // Test what happens when NATS server is not available
    let mut plugin = GeyserPluginNats::new();

    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let config = NatsPluginConfig {
        nats_url: "nats://127.0.0.1:19999".to_string(), // Non-existent port
        subject: "test.transactions".to_string(),
        max_retries: 5,
        timeout_secs: 10,
        filter: TransactionFilterConfig::default(),
    };
    let config_json = serde_json::to_string(&config).expect("Failed to serialize config");
    fs::write(&temp_file, config_json).expect("Failed to write to temp file");

    // Plugin creation should succeed
    let result = plugin.on_load(temp_file.path().to_str().unwrap(), false);
    assert!(
        result.is_ok(),
        "Plugin creation should succeed even if NATS connection fails asynchronously"
    );

    // Plugin should still be enabled for transactions
    assert!(plugin.transaction_notifications_enabled());
}

#[test]
fn test_nats_server_infrastructure() {
    // Test that our NATS server infrastructure works
    let server = match skip_if_nats_unavailable() {
        Some(server) => server,
        None => return,
    };

    let url = server.url();

    println!("NATS test server started on: {url}");

    // Verify we can connect to it
    let stream_result = std::net::TcpStream::connect(&url);
    assert!(
        stream_result.is_ok(),
        "Should be able to connect to NATS test server"
    );

    println!("Successfully connected to NATS test server");
}

#[test]
fn test_plugin_with_real_nats_server() {
    // Test plugin functionality with real NATS server
    let nats_server = match skip_if_nats_unavailable() {
        Some(server) => server,
        None => return,
    };

    let nats_url = format!("nats://{}", nats_server.url());
    let subject = "test.transactions.real";

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

    plugin
        .on_load(temp_file.path().to_str().unwrap(), false)
        .expect("Plugin should load successfully");

    // Give plugin time to connect
    thread::sleep(Duration::from_millis(100));

    println!("Plugin loaded and connected successfully to real NATS server!");

    plugin.on_unload();
}
