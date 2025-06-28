use {
    crossbeam_channel::{Receiver, Sender},
    log::{debug, error, info},
    std::{
        io::{BufRead, BufReader, BufWriter, Write},
        net::{SocketAddr, TcpStream, ToSocketAddrs},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
        time::Duration,
    },
    thiserror::Error,
};

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("Failed to connect to NATS server: {msg}")]
    ConnectionFailed { msg: String },

    #[error("Failed to resolve hostname: {msg}")]
    HostResolutionFailed { msg: String },

    #[error("Connection lost: {msg}")]
    ConnectionLost { msg: String },

    #[error("Failed to send message: {msg}")]
    SendFailed { msg: String },
}

#[derive(Debug, Clone)]
pub struct NatsMessage {
    pub subject: String,
    pub payload: Vec<u8>,
}

pub struct ConnectionManager {
    sender: Sender<NatsMessage>,
    shutdown: Arc<AtomicBool>,
    worker_handle: Option<thread::JoinHandle<()>>,
}

impl ConnectionManager {
    /// Create a new connection with the specified NATS server address
    pub fn new(
        nats_url: &str,
        max_retries: u32,
        timeout_secs: u64,
    ) -> Result<Self, ConnectionError> {
        info!("Creating NATS connection to: {nats_url}");

        let addr = Self::resolve_nats_address(nats_url)?;
        let (sender, receiver) = crossbeam_channel::unbounded::<NatsMessage>();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        // Spawn worker thread to handle NATS connection
        let worker_handle = thread::spawn(move || {
            Self::connection_worker(addr, receiver, shutdown_clone, max_retries, timeout_secs);
        });

        info!("NATS connection created successfully");

        Ok(Self {
            sender,
            shutdown,
            worker_handle: Some(worker_handle),
        })
    }

    /// Resolve NATS URL to socket address
    fn resolve_nats_address(nats_url: &str) -> Result<SocketAddr, ConnectionError> {
        let host_port = nats_url.replace("nats://", "");
        let parts: Vec<&str> = host_port.split(':').collect();

        if parts.len() != 2 {
            return Err(ConnectionError::HostResolutionFailed {
                msg: format!("Invalid NATS URL format: {nats_url}"),
            });
        }

        let host = parts[0];
        let port: u16 = parts[1]
            .parse()
            .map_err(|e| ConnectionError::HostResolutionFailed {
                msg: format!("Invalid port number: {e}"),
            })?;

        info!("Resolving NATS host: {host} port: {port}");

        let addr = format!("{host}:{port}")
            .to_socket_addrs()
            .map_err(|e| ConnectionError::HostResolutionFailed {
                msg: format!("Failed to resolve hostname {host}: {e}"),
            })?
            .next()
            .ok_or_else(|| ConnectionError::HostResolutionFailed {
                msg: format!("No addresses found for hostname: {host}"),
            })?;

        Ok(addr)
    }

    /// Send a message through the NATS connection
    pub fn send_message(&self, message: NatsMessage) -> Result<(), ConnectionError> {
        self.sender
            .send(message)
            .map_err(|e| ConnectionError::SendFailed {
                msg: format!("Failed to queue message: {e}"),
            })
    }

    /// Worker thread that maintains the NATS connection and processes messages
    fn connection_worker(
        addr: SocketAddr,
        receiver: Receiver<NatsMessage>,
        shutdown: Arc<AtomicBool>,
        max_retries: u32,
        timeout_secs: u64,
    ) {
        let mut retry_count = 0;
        let timeout = Duration::from_secs(timeout_secs);

        while !shutdown.load(Ordering::Relaxed) && retry_count < max_retries {
            match TcpStream::connect_timeout(&addr, timeout) {
                Ok(stream) => {
                    info!("Connected to NATS server at {addr}");
                    retry_count = 0; // Reset retry count on successful connection

                    if let Err(e) = Self::handle_connection(stream, &receiver, &shutdown) {
                        error!("NATS connection error: {e}");
                    }
                }
                Err(e) => {
                    retry_count += 1;
                    error!("Failed to connect to NATS (attempt {retry_count}/{max_retries}): {e}");

                    if retry_count < max_retries {
                        thread::sleep(Duration::from_secs(2_u64.pow(retry_count.min(5))));
                    }
                }
            }
        }

        if retry_count >= max_retries {
            error!("Max connection retries ({max_retries}) exceeded. Giving up.");
        }

        info!("NATS connection worker thread shutting down");
    }

    /// Handle a single NATS connection session
    fn handle_connection(
        stream: TcpStream,
        receiver: &Receiver<NatsMessage>,
        shutdown: &Arc<AtomicBool>,
    ) -> Result<(), ConnectionError> {
        let mut reader =
            BufReader::new(
                stream
                    .try_clone()
                    .map_err(|e| ConnectionError::ConnectionLost {
                        msg: format!("Failed to clone stream: {e}"),
                    })?,
            );
        let mut writer = BufWriter::new(stream);

        // Send CONNECT command
        Self::write_command(
            &mut writer,
            "CONNECT {\"verbose\":false,\"pedantic\":false,\"name\":\"solana-geyser-nats\"}",
        )
        .map_err(|e| ConnectionError::ConnectionLost {
            msg: format!("Failed to send CONNECT command: {e}"),
        })?;

        // Send initial PING
        Self::write_command(&mut writer, "PING").map_err(|e| ConnectionError::ConnectionLost {
            msg: format!("Failed to send initial PING: {e}"),
        })?;
        writer
            .flush()
            .map_err(|e| ConnectionError::ConnectionLost {
                msg: format!("Failed to flush initial commands: {e}"),
            })?;

        // Read initial responses
        Self::read_response(&mut reader)?;

        // Main message processing loop
        let mut last_ping = std::time::Instant::now();
        let ping_interval = Duration::from_secs(30);

        while !shutdown.load(Ordering::Relaxed) {
            // Process any queued messages
            match receiver.try_recv() {
                Ok(msg) => {
                    Self::write_publish_message(&mut writer, &msg).map_err(|e| {
                        ConnectionError::SendFailed {
                            msg: format!("Failed to publish message: {e}"),
                        }
                    })?;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    // No messages, check if we need to ping
                    if last_ping.elapsed() >= ping_interval {
                        Self::write_command(&mut writer, "PING").map_err(|e| {
                            ConnectionError::ConnectionLost {
                                msg: format!("Failed to send keepalive PING: {e}"),
                            }
                        })?;
                        writer
                            .flush()
                            .map_err(|e| ConnectionError::ConnectionLost {
                                msg: format!("Failed to flush keepalive PING: {e}"),
                            })?;
                        last_ping = std::time::Instant::now();
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    info!("Message channel disconnected, closing connection");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Write a NATS publish message to a writer
    fn write_publish_message<W: Write>(
        writer: &mut BufWriter<W>,
        msg: &NatsMessage,
    ) -> Result<(), std::io::Error> {
        // PUB subject
        let command = format!("PUB {} {}\r\n", msg.subject, msg.payload.len());
        writer.write_all(command.as_bytes())?;

        // payload
        writer.write_all(&msg.payload)?;
        writer.write_all(b"\r\n")?;
        writer.flush()?;

        debug!("Published NATS message: {} bytes", msg.payload.len());
        Ok(())
    }

    /// Write a NATS command to a writer
    fn write_command<W: Write>(
        writer: &mut BufWriter<W>,
        command: &str,
    ) -> Result<(), std::io::Error> {
        let formatted = format!("{command}\r\n");
        writer.write_all(formatted.as_bytes())?;
        Ok(())
    }

    /// Read and discard a response from the NATS server
    fn read_response(reader: &mut BufReader<TcpStream>) -> Result<(), ConnectionError> {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|e| ConnectionError::ConnectionLost {
                msg: format!("Failed to read NATS response: {e}"),
            })?;
        debug!("NATS server response: {}", line.trim());
        Ok(())
    }

    /// Shutdown the connection manager
    pub fn shutdown(&mut self) {
        info!("Shutting down NATS connection manager");
        self.shutdown.store(true, Ordering::Relaxed);

        if let Some(handle) = self.worker_handle.take() {
            if let Err(e) = handle.join() {
                error!("Error joining worker thread: {e:?}");
            }
        }
    }
}

impl Drop for ConnectionManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}
