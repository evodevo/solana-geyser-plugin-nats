use std::{
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct NatsTestServer {
    process: Option<Child>,
    port: u16,
}

#[derive(Debug)]
pub enum NatsServerError {
    BinaryNotFound,
    StartupTimeout,
    Other(String),
}

impl std::fmt::Display for NatsServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NatsServerError::BinaryNotFound => write!(
                f,
                "nats-server binary not found. Please install nats-server to run this test."
            ),
            NatsServerError::StartupTimeout => {
                write!(f, "NATS server failed to start within timeout")
            }
            NatsServerError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for NatsServerError {}

impl NatsTestServer {
    pub fn start() -> Result<Self, NatsServerError> {
        // Find an available port
        let port = find_available_port()?;

        // Try to start nats-server binary
        let process = Command::new("nats-server")
            .args([
                "--port",
                &port.to_string(),
                "--jetstream",
                "false",
                "--log_file",
                "/dev/null",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    NatsServerError::BinaryNotFound
                } else {
                    NatsServerError::Other(format!("Failed to start nats-server: {e}"))
                }
            })?;

        let server = NatsTestServer {
            process: Some(process),
            port,
        };

        // Wait for server to be ready
        server.wait_for_ready()?;

        Ok(server)
    }

    pub fn url(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    fn wait_for_ready(&self) -> Result<(), NatsServerError> {
        let start = Instant::now();
        let timeout = Duration::from_secs(10);

        while start.elapsed() < timeout {
            if TcpStream::connect(("127.0.0.1", self.port)).is_ok() {
                thread::sleep(Duration::from_millis(100)); // Give it a bit more time
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }

        Err(NatsServerError::StartupTimeout)
    }
}

impl Drop for NatsTestServer {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

fn find_available_port() -> Result<u16, NatsServerError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| NatsServerError::Other(format!("Failed to bind to port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| NatsServerError::Other(format!("Failed to get local address: {e}")))?
        .port();
    drop(listener);
    Ok(port)
}
