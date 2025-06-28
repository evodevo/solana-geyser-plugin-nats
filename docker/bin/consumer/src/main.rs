use anyhow::Result;
use async_nats::{Client, Message};
use clap::Parser;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
use tracing::error;

#[derive(Parser, Debug)]
#[command(name = "nats-consumer")]
#[command(about = "NATS Consumer for Solana transactions")]
struct Args {
    #[arg(long, default_value = "nats://nats:4222")]
    nats_url: String,

    #[arg(long, default_value = "solana.transactions.non_vote")]
    subject: String,

    #[arg(long, default_value = "/app/data")]
    data_dir: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct ReceivedMessage {
    timestamp: String,
    subject: String,
    data: Value,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let mut args = Args::parse();
    
    // Override with environment variables if present
    if let Ok(nats_url) = std::env::var("NATS_URL") {
        args.nats_url = nats_url;
    }
    if let Ok(subject) = std::env::var("NATS_SUBJECT") {
        args.subject = subject;
    }

    println!("================================================================================");
    println!("NATS-CONSUMER: Starting NATS Consumer...");
    println!("NATS-CONSUMER: NATS URL: {}", args.nats_url);
    println!("NATS-CONSUMER: Subject: {}", args.subject);
    println!("================================================================================");

    // Create data directory
    fs::create_dir_all(&args.data_dir)?;

    let mut consumer = NatsConsumer::new(args.nats_url, args.subject, args.data_dir).await?;
    consumer.run().await?;

    Ok(())
}

struct NatsConsumer {
    client: Client,
    subject: String,
    data_dir: String,
    messages: Vec<ReceivedMessage>,
}

impl NatsConsumer {
    async fn new(nats_url: String, subject: String, data_dir: String) -> Result<Self> {
        // Connect to NATS
        let client = Self::connect_with_retry(&nats_url).await?;

        Ok(Self {
            client,
            subject,
            data_dir,
            messages: Vec::new(),
        })
    }

    async fn connect_with_retry(nats_url: &str) -> Result<Client> {
        const MAX_RETRIES: u32 = 30;
        let mut retry_count = 0;

        loop {
            match async_nats::connect(nats_url).await {
                Ok(client) => {
                    println!("NATS-CONSUMER: Connected to NATS at {}", nats_url);
                    return Ok(client);
                }
                Err(e) => {
                    retry_count += 1;
                    println!(
                        "NATS-CONSUMER: Connection attempt {}/{} failed: {}",
                        retry_count, MAX_RETRIES, e
                    );

                    if retry_count >= MAX_RETRIES {
                        return Err(anyhow::anyhow!("Failed to connect to NATS after {} retries", MAX_RETRIES));
                    }

                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    async fn run(&mut self) -> Result<()> {
        // Subscribe to the subject
        let mut subscriber = self.client.subscribe(self.subject.clone()).await?;
        println!("NATS-CONSUMER: Subscribed to subject: {}", self.subject);

        // Create ready file
        let ready_file = Path::new(&self.data_dir).join("consumer_ready");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();
        fs::write(&ready_file, format!("Consumer ready at {}", timestamp))?;

        println!("NATS-CONSUMER: Ready and waiting for messages...");

        // Process messages
        while let Some(message) = subscriber.next().await {
            if let Err(e) = self.handle_message(message).await {
                error!("NATS-CONSUMER: Error handling message: {}", e);
            }
        }

        Ok(())
    }

    async fn handle_message(&mut self, msg: Message) -> Result<()> {
        println!("NATS-CONSUMER: MESSAGE RECEIVED!");
        println!("NATS-CONSUMER:    Subject: {}", msg.subject);
        println!("NATS-CONSUMER:    Message size: {} bytes", msg.payload.len());

        // Decode and parse the message
        let raw_data = String::from_utf8(msg.payload.to_vec())?;
        println!("NATS-CONSUMER:    Raw data: {}", raw_data);

        let message_data: Value = serde_json::from_str(&raw_data)?;

        // Create received message record
        let received_message = ReceivedMessage {
            timestamp: chrono::Utc::now().to_rfc3339(),
            subject: msg.subject.to_string(),
            data: message_data.clone(),
        };

        // Store the message
        self.messages.push(received_message);

        // Save to file immediately
        self.save_messages().await?;

        println!("NATS-CONSUMER: Successfully processed message #{}", self.messages.len());
        println!("NATS-CONSUMER:    Subject: {}", msg.subject);

        // Extract transaction info
        if let Some(transaction) = message_data.get("transaction") {
            if let Some(signatures) = transaction.get("signatures") {
                println!("NATS-CONSUMER:    Transaction signatures: {}", signatures);
            }
        }

        if let Some(slot) = message_data.get("slot") {
            println!("NATS-CONSUMER:    Slot: {}", slot);
        }

        if let Some(block_time) = message_data.get("blockTime") {
            println!("NATS-CONSUMER:    Block time: {}", block_time);
        }

        // Check for expected transaction signatures
        let expected_sigs = vec![
            "3fBuLTcfbh9du8STM4MPfna8VPY8c9mWTKKcGKieS5htqzkDRJn8i8ssCpmWVMUVcEddsNUiT8esFZvZ5N36PPpC",
            "CkFjm2udqa8RQhxpkPBsN4nxwDWETuwDJpT1CrrLgw4DSAmHCRJbHs9MtUHam3SwVwHiZkB7wfvNteRCHC2oMf1",
            "3GWnAAtiEP6xZpUTTAYcrztqEhoyCsamN3MDVDjgRFq5KLMkLJTtgHDsgSRZtUnSJY6J2qDmwVc5EiXaNzcT63WY",
        ];

        if let Some(transaction) = message_data.get("transaction") {
            if let Some(signatures) = transaction.get("signatures").and_then(|s| s.as_array()) {
                for sig in signatures {
                    if let Some(sig_str) = sig.as_str() {
                        for expected_sig in &expected_sigs {
                            if expected_sig.starts_with(&sig_str[..sig_str.len().min(10)]) {
                                println!("NATS-CONSUMER: Found matching transaction: {}", sig_str);
                            }
                        }
                    }
                }
            }
        }

        println!("NATS-CONSUMER: {}", "=".repeat(80));

        Ok(())
    }

    async fn save_messages(&self) -> Result<()> {
        let messages_file = Path::new(&self.data_dir).join("received_messages.json");
        let json_data = serde_json::to_string_pretty(&self.messages)?;
        fs::write(&messages_file, json_data)?;
        Ok(())
    }
} 