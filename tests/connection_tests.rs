use {
    solana_geyser_plugin_nats::connection::{ConnectionError, ConnectionManager, NatsMessage},
    std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        sync::Arc,
        thread,
        time::Duration,
    },
};

fn create_test_message() -> NatsMessage {
    NatsMessage {
        subject: "test.subject".to_string(),
        payload: b"test payload".to_vec(),
    }
}

fn create_test_message_with_subject(subject: &str) -> NatsMessage {
    NatsMessage {
        subject: subject.to_string(),
        payload: b"test payload".to_vec(),
    }
}

// Mock NATS server for testing actual protocol behavior
struct MockNatsServer {
    listener: TcpListener,
    port: u16,
}

impl MockNatsServer {
    fn new() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        Ok(Self { listener, port })
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn run_simple_response_server(&self) -> thread::JoinHandle<()> {
        let listener = self.listener.try_clone().unwrap();
        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let mut read_stream = stream.try_clone().unwrap();
                let mut write_stream = stream;
                let mut reader = BufReader::new(&mut read_stream);
                let mut line = String::new();

                // Send INFO message
                let _ = write_stream.write_all(b"INFO {\"server_id\":\"test\"}\r\n");

                // Read and respond to commands
                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    if line.trim().starts_with("CONNECT") {
                        let _ = write_stream.write_all(b"+OK\r\n");
                    } else if line.trim().starts_with("PUB") {
                        // Read the payload length and consume payload
                        if let Some(parts) = line.split_whitespace().nth(2) {
                            if let Ok(payload_len) = parts.parse::<usize>() {
                                let mut payload = vec![0u8; payload_len + 2]; // +2 for \r\n
                                let _ = reader.read_exact(&mut payload);
                            }
                        }
                        let _ = write_stream.write_all(b"+OK\r\n");
                    } else if line.trim() == "PING" {
                        let _ = write_stream.write_all(b"PONG\r\n");
                    }
                    line.clear();
                }
            }
        })
    }

    fn run_error_response_server(&self) -> thread::JoinHandle<()> {
        let listener = self.listener.try_clone().unwrap();
        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let mut read_stream = stream.try_clone().unwrap();
                let mut write_stream = stream;
                let mut reader = BufReader::new(&mut read_stream);
                let mut line = String::new();

                // Send INFO and then error responses
                let _ = write_stream.write_all(b"INFO {\"server_id\":\"test\"}\r\n");

                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    let _ = write_stream.write_all(b"-ERR 'Test Error'\r\n");
                    line.clear();
                }
            }
        })
    }

    fn run_slow_response_server(&self, delay_ms: u64) -> thread::JoinHandle<()> {
        let listener = self.listener.try_clone().unwrap();
        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let mut read_stream = stream.try_clone().unwrap();
                let mut write_stream = stream;
                let mut reader = BufReader::new(&mut read_stream);
                let mut line = String::new();

                let _ = write_stream.write_all(b"INFO {\"server_id\":\"test\"}\r\n");

                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    thread::sleep(Duration::from_millis(delay_ms));
                    let _ = write_stream.write_all(b"+OK\r\n");
                    line.clear();
                }
            }
        })
    }
}

#[cfg(test)]
mod mock_server_tests {
    use super::*;

    #[test]
    fn test_successful_connection_and_protocol_handshake() {
        // This test exercises handle_connection, write_command, read_response
        let mock_server = MockNatsServer::new().unwrap();
        let port = mock_server.port();
        let _server_handle = mock_server.run_simple_response_server();

        thread::sleep(Duration::from_millis(50));

        let mut manager =
            ConnectionManager::new(&format!("nats://127.0.0.1:{port}"), 5, 2).unwrap();

        let msg = create_test_message_with_subject("test.protocol.handshake");
        assert!(manager.send_message(msg).is_ok());

        thread::sleep(Duration::from_millis(200));
        manager.shutdown();
    }

    #[test]
    fn test_write_publish_message_coverage() {
        // This test specifically exercises write_publish_message
        let mock_server = MockNatsServer::new().unwrap();
        let port = mock_server.port();
        let _server_handle = mock_server.run_simple_response_server();

        thread::sleep(Duration::from_millis(50));

        let mut manager =
            ConnectionManager::new(&format!("nats://127.0.0.1:{port}"), 3, 2).unwrap();

        // Test different message formats to exercise protocol formatting
        let test_messages = vec![
            NatsMessage {
                subject: "short".to_string(),
                payload: b"x".to_vec(),
            },
            NatsMessage {
                subject: "test.very.long.subject.name".to_string(),
                payload: b"some payload".to_vec(),
            },
            NatsMessage {
                subject: "empty.payload".to_string(),
                payload: vec![],
            },
            NatsMessage {
                subject: "binary.data".to_string(),
                payload: vec![0, 1, 2, 255],
            },
        ];

        for msg in test_messages {
            assert!(manager.send_message(msg).is_ok());
            thread::sleep(Duration::from_millis(10));
        }

        thread::sleep(Duration::from_millis(200));
        manager.shutdown();
    }

    #[test]
    fn test_connection_error_handling_paths() {
        // Test error response handling from server
        let mock_server = MockNatsServer::new().unwrap();
        let port = mock_server.port();
        let _server_handle = mock_server.run_error_response_server();

        thread::sleep(Duration::from_millis(50));

        let mut manager =
            ConnectionManager::new(&format!("nats://127.0.0.1:{port}"), 2, 1).unwrap();

        let msg = create_test_message_with_subject("test.error.response");
        assert!(manager.send_message(msg).is_ok());

        thread::sleep(Duration::from_millis(200));
        manager.shutdown();
    }

    #[test]
    fn test_keepalive_ping_coverage() {
        // Test the keepalive PING logic by keeping connection alive
        let mock_server = MockNatsServer::new().unwrap();
        let port = mock_server.port();
        let _server_handle = mock_server.run_simple_response_server();

        thread::sleep(Duration::from_millis(50));

        let mut manager =
            ConnectionManager::new(&format!("nats://127.0.0.1:{port}"), 5, 3).unwrap();

        let msg = create_test_message_with_subject("test.keepalive.initial");
        assert!(manager.send_message(msg).is_ok());

        // Keep connection active to trigger ping logic
        for i in 0..3 {
            thread::sleep(Duration::from_millis(100));
            let msg = create_test_message_with_subject(&format!("test.keepalive.{i}"));
            let _ = manager.send_message(msg);
        }

        manager.shutdown();
    }

    #[test]
    fn test_slow_server_response_handling() {
        // Test timeout handling and slow responses
        let mock_server = MockNatsServer::new().unwrap();
        let port = mock_server.port();
        let _server_handle = mock_server.run_slow_response_server(100);

        thread::sleep(Duration::from_millis(50));

        let mut manager =
            ConnectionManager::new(&format!("nats://127.0.0.1:{port}"), 3, 1).unwrap();

        let msg = create_test_message_with_subject("test.slow.response");
        assert!(manager.send_message(msg).is_ok());

        thread::sleep(Duration::from_millis(500));
        manager.shutdown();
    }

    #[test]
    fn test_connection_recovery_after_failure() {
        // Test connection recovery logic
        let mock_server = MockNatsServer::new().unwrap();
        let port = mock_server.port();

        let error_handle = mock_server.run_error_response_server();
        thread::sleep(Duration::from_millis(50));

        let mut manager =
            ConnectionManager::new(&format!("nats://127.0.0.1:{port}"), 10, 1).unwrap();

        let msg = create_test_message_with_subject("test.recovery.initial");
        assert!(manager.send_message(msg).is_ok());

        thread::sleep(Duration::from_millis(200));

        // Simulate server restart
        drop(error_handle);
        let _good_handle = mock_server.run_simple_response_server();

        for i in 0..2 {
            let msg = create_test_message_with_subject(&format!("test.recovery.{i}"));
            let _ = manager.send_message(msg);
            thread::sleep(Duration::from_millis(50));
        }

        manager.shutdown();
    }

    #[test]
    fn test_large_message_protocol_handling() {
        // Test handling of large messages through protocol
        let mock_server = MockNatsServer::new().unwrap();
        let port = mock_server.port();
        let _server_handle = mock_server.run_simple_response_server();

        thread::sleep(Duration::from_millis(50));

        let mut manager =
            ConnectionManager::new(&format!("nats://127.0.0.1:{port}"), 3, 2).unwrap();

        // Large message to exercise protocol formatting
        let large_payload = vec![0x42; 50_000]; // 50KB message
        let msg = NatsMessage {
            subject: "test.large.message".to_string(),
            payload: large_payload,
        };

        assert!(manager.send_message(msg).is_ok());
        thread::sleep(Duration::from_millis(300));

        manager.shutdown();
    }

    #[test]
    fn test_concurrent_messages_with_successful_connection() {
        // Test concurrent message sending with actual connection
        let mock_server = MockNatsServer::new().unwrap();
        let port = mock_server.port();
        let _server_handle = mock_server.run_simple_response_server();

        thread::sleep(Duration::from_millis(50));

        let manager =
            Arc::new(ConnectionManager::new(&format!("nats://127.0.0.1:{port}"), 5, 2).unwrap());

        let mut handles = vec![];

        for thread_id in 0..3 {
            let manager_clone = manager.clone();
            let handle = thread::spawn(move || {
                for msg_id in 0..5 {
                    let msg = create_test_message_with_subject(&format!(
                        "test.concurrent.{thread_id}.{msg_id}"
                    ));
                    let _ = manager_clone.send_message(msg);
                    thread::sleep(Duration::from_millis(10));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        thread::sleep(Duration::from_millis(200));
    }

    #[test]
    fn test_connection_manager_creation_with_invalid_url() {
        let result = ConnectionManager::new("invalid-url", 1, 1);
        assert!(result.is_err());
        if let Err(ConnectionError::HostResolutionFailed { msg }) = result {
            assert!(msg.contains("Invalid NATS URL format"));
        }
    }

    #[test]
    fn test_connection_manager_creation_with_nonexistent_host() {
        let result = ConnectionManager::new("nats://nonexistent.invalid.hostname.test:4222", 1, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_connection_manager_creation_success() {
        // Should succeed in creation even if no server running
        let result = ConnectionManager::new("nats://127.0.0.1:4222", 3, 2);
        assert!(result.is_ok());

        let mut manager = result.unwrap();
        manager.shutdown();
    }

    #[test]
    fn test_send_message_basic() {
        let mut manager = ConnectionManager::new("nats://127.0.0.1:9999", 1, 1).unwrap();

        let msg = create_test_message();
        assert!(manager.send_message(msg).is_ok());

        manager.shutdown();
    }

    #[test]
    fn test_send_message_after_shutdown() {
        let mut manager = ConnectionManager::new("nats://127.0.0.1:9999", 1, 1).unwrap();

        manager.shutdown();

        let msg = create_test_message();
        assert!(manager.send_message(msg).is_err());
    }

    #[test]
    fn test_connection_error_display() {
        let error = ConnectionError::HostResolutionFailed {
            msg: "Test error".to_string(),
        };

        let display_string = format!("{error}");
        assert!(display_string.contains("Test error"));
    }
}
