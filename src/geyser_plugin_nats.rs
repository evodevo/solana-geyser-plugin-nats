use {
    crate::{
        config::{ConfigurationManager, NatsPluginConfig},
        connection::ConnectionManager,
        processor::TransactionProcessor,
    },
    agave_geyser_plugin_interface::geyser_plugin_interface::{
        GeyserPlugin, GeyserPluginError, ReplicaAccountInfoVersions, ReplicaBlockInfoVersions,
        ReplicaTransactionInfoVersions, Result, SlotStatus,
    },
    log::{debug, error, info},
    std::sync::Arc,
};

#[derive(Default)]
pub struct GeyserPluginNats {
    processor: Option<Arc<TransactionProcessor>>,
    connection_manager: Option<Arc<ConnectionManager>>,
}

impl std::fmt::Debug for GeyserPluginNats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeyserPluginNats")
            .field("processor_initialized", &self.processor.is_some())
            .field("connection_initialized", &self.connection_manager.is_some())
            .finish()
    }
}

impl GeyserPlugin for GeyserPluginNats {
    fn name(&self) -> &'static str {
        "GeyserPluginNats"
    }

    /// Initialize the NATS plugin
    fn on_load(&mut self, config_file: &str, _is_reload: bool) -> Result<()> {
        solana_logger::setup_with_default("info");
        info!(
            "Loading plugin {:?} from config_file {:?}",
            self.name(),
            config_file
        );

        // Load and validate configuration
        let config = ConfigurationManager::load_config(config_file).map_err(|err| match err {
            crate::config::ConfigError::FileReadError { msg } => {
                GeyserPluginError::ConfigFileReadError { msg }
            }
            crate::config::ConfigError::ParseError { msg } => {
                GeyserPluginError::ConfigFileReadError { msg }
            }
            crate::config::ConfigError::ValidationError { msg } => {
                GeyserPluginError::ConfigFileReadError { msg }
            }
        })?;

        info!("Configuration loaded successfully");

        let (connection_manager, processor) = Self::initialize_components(config)?;

        self.connection_manager = Some(connection_manager);
        self.processor = Some(processor);

        info!("NATS plugin successfully loaded and connected");
        Ok(())
    }

    fn on_unload(&mut self) {
        info!("Unloading plugin: {}", self.name());

        // Clean shutdown
        let connection_manager = self.connection_manager.take();
        if let Err(e) = Self::shutdown_components(connection_manager) {
            error!("Error during shutdown: {e}");
        }

        self.processor = None;

        info!("Plugin unloaded successfully");
    }

    fn update_account(
        &self,
        _account: ReplicaAccountInfoVersions,
        _slot: u64,
        _is_startup: bool,
    ) -> Result<()> {
        // Do not process any accounts
        Ok(())
    }

    fn update_slot_status(
        &self,
        _slot: u64,
        _parent: Option<u64>,
        _status: &SlotStatus,
    ) -> Result<()> {
        // Do not process any slot status
        Ok(())
    }

    fn notify_end_of_startup(&self) -> Result<()> {
        info!("NATS plugin startup complete");
        Ok(())
    }

    fn notify_transaction(
        &self,
        transaction_info: ReplicaTransactionInfoVersions,
        slot: u64,
    ) -> Result<()> {
        let processor = self.processor.as_ref().ok_or_else(|| {
            GeyserPluginError::Custom(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Transaction processor not initialized",
            )))
        })?;

        processor
            .process_transaction(transaction_info, slot)
            .map_err(|err| {
                error!("Failed to process transaction: {err:?}");
                GeyserPluginError::Custom(Box::new(err))
            })
    }

    fn notify_block_metadata(&self, _block_info: ReplicaBlockInfoVersions) -> Result<()> {
        // Do not process block metadata
        Ok(())
    }

    fn account_data_notifications_enabled(&self) -> bool {
        // Do not process any account data
        false
    }

    fn transaction_notifications_enabled(&self) -> bool {
        self.processor
            .as_ref()
            .is_some_and(|processor| processor.is_enabled())
    }
}

impl GeyserPluginNats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialize all plugin components from configuration
    fn initialize_components(
        config: NatsPluginConfig,
    ) -> Result<(Arc<ConnectionManager>, Arc<TransactionProcessor>)> {
        info!("Initializing NATS plugin");
        debug!("Config: {config:?}");

        // Create connection manager
        let connection_manager = Arc::new(
            ConnectionManager::new(&config.nats_url, config.max_retries, config.timeout_secs)
                .map_err(|err| GeyserPluginError::Custom(Box::new(err)))?,
        );

        // Create transaction processor
        let processor = Arc::new(TransactionProcessor::new(
            connection_manager.clone(),
            &config.filter,
            config.subject.clone(),
        ));

        info!("NATS plugin initialized successfully");
        Ok((connection_manager, processor))
    }

    /// Shutdown all plugin components gracefully
    fn shutdown_components(connection_manager: Option<Arc<ConnectionManager>>) -> Result<()> {
        info!("Shutting down plugin");

        if let Some(mut connection_manager) = connection_manager {
            if let Some(manager) = Arc::get_mut(&mut connection_manager) {
                manager.shutdown();
            }
        }

        info!("Plugin shut down successfully");
        Ok(())
    }
}

#[no_mangle]
#[allow(improper_ctypes_definitions)]
/// This function returns the GeyserPluginNats pointer as trait GeyserPlugin.
///
/// # Safety
/// This function is called by the Solana validator to create a plugin instance.
/// The returned pointer must be used correctly by the caller to avoid memory leaks
/// and use-after-free errors. The caller is responsible for managing the plugin's lifecycle.
pub unsafe extern "C" fn _create_plugin() -> *mut dyn GeyserPlugin {
    let plugin = GeyserPluginNats::new();
    let plugin: Box<dyn GeyserPlugin> = Box::new(plugin);
    Box::into_raw(plugin)
}
