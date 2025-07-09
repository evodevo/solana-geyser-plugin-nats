use {
    log::debug,
    serde_derive::{Deserialize, Serialize},
    std::{fs::File, io::Read},
    thiserror::Error,
};

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {msg}")]
    FileReadError { msg: String },

    #[error("Failed to parse config: {msg}")]
    ParseError { msg: String },

    #[error("Invalid configuration: {msg}")]
    ValidationError { msg: String },
}

/// Configuration for the NATS Geyser Plugin
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NatsPluginConfig {
    /// The NATS server URL (e.g., "nats://localhost:4222")
    pub nats_url: String,

    /// The NATS subject to publish transactions to
    pub subject: String,

    /// Optional: Maximum number of connection retries
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Optional: Connection timeout in seconds
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,

    /// Optional: Filter configuration
    #[serde(default)]
    pub filter: TransactionFilterConfig,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransactionFilterConfig {
    /// Whether to process all transactions (except voting)
    #[serde(default)]
    pub select_all_transactions: bool,

    /// Whether to process vote transactions
    #[serde(default)]
    pub select_vote_transactions: bool,

    /// Specific addresses to include (empty includes all)
    #[serde(default)]
    pub mentioned_addresses: Vec<String>,
}

impl Default for TransactionFilterConfig {
    fn default() -> Self {
        Self {
            select_all_transactions: true,
            select_vote_transactions: false,
            mentioned_addresses: vec![],
        }
    }
}

fn default_max_retries() -> u32 {
    5
}

fn default_timeout_secs() -> u64 {
    10
}

pub struct ConfigurationManager;

impl ConfigurationManager {
    /// Load and validate configuration from file
    pub fn load_config(config_file: &str) -> Result<NatsPluginConfig, ConfigError> {
        let mut file = File::open(config_file).map_err(|err| ConfigError::FileReadError {
            msg: format!("Failed to open config file '{config_file}': {err}"),
        })?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(|err| ConfigError::FileReadError {
                msg: format!("Failed to read config file '{config_file}': {err}"),
            })?;

        let config: NatsPluginConfig =
            serde_json::from_str(&contents).map_err(|err| ConfigError::ParseError {
                msg: format!("Failed to parse JSON config from '{config_file}': {err}"),
            })?;

        Self::validate_config(&config)?;
        Ok(config)
    }

    /// Validate all configuration values
    fn validate_config(config: &NatsPluginConfig) -> Result<(), ConfigError> {
        debug!("Validating configuration: {config:?}");

        Self::validate_nats_url(&config.nats_url)?;
        Self::validate_subject(&config.subject)?;
        Self::validate_timeout(config.timeout_secs)?;
        Self::validate_mentioned_addresses(&config.filter.mentioned_addresses)?;

        debug!("Configuration validation successful");
        Ok(())
    }

    /// Validate NATS URL
    fn validate_nats_url(nats_url: &str) -> Result<(), ConfigError> {
        if !nats_url.starts_with("nats://") {
            return Err(ConfigError::ValidationError {
                msg: format!(
                    "Invalid NATS URL format: '{nats_url}'. Expected format: nats://host:port"
                ),
            });
        }

        // Check if NATS URL can be parsed
        let host_port = nats_url.replace("nats://", "");
        let parts: Vec<&str> = host_port.split(':').collect();
        if parts.len() != 2 {
            return Err(ConfigError::ValidationError {
                msg: format!(
                    "Invalid NATS URL format: '{nats_url}'. Expected format: nats://host:port"
                ),
            });
        }

        // Check if NATS port is a number
        if parts[1].parse::<u16>().is_err() {
            return Err(ConfigError::ValidationError {
                msg: format!("Invalid port number in NATS URL: '{}'", parts[1]),
            });
        }

        Ok(())
    }

    /// Validate NATS subject
    fn validate_subject(subject: &str) -> Result<(), ConfigError> {
        if subject.trim().is_empty() {
            return Err(ConfigError::ValidationError {
                msg: "NATS subject cannot be empty".to_string(),
            });
        }

        Ok(())
    }

    /// Validate timeout settings
    fn validate_timeout(timeout_secs: u64) -> Result<(), ConfigError> {
        if timeout_secs == 0 || timeout_secs > 300 {
            return Err(ConfigError::ValidationError {
                msg: format!("Invalid timeout: {timeout_secs} seconds. Must be between 1 and 300"),
            });
        }

        Ok(())
    }

    /// Validate mentioned addresses if provided
    fn validate_mentioned_addresses(addresses: &[String]) -> Result<(), ConfigError> {
        for address in addresses {
            if address != "*"
                && address != "all"
                && address != "all_votes"
                && bs58::decode(address).into_vec().is_err()
            {
                return Err(ConfigError::ValidationError {
                    msg: format!("Invalid base58 address: '{address}'"),
                });
            }
        }

        Ok(())
    }
}
