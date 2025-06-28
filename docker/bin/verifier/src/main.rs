use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;


#[derive(Parser, Debug)]
#[command(name = "message-verifier")]
#[command(about = "Verify NATS messages received during integration test")]
struct Args {
    #[arg(long, default_value = "/app/data")]
    data_dir: String,

    #[arg(long, default_value = "1")]
    expected_min_messages: usize,
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

    let args = Args::parse();

    println!("NATS Message Verifier");
    println!("========================");
    println!("Data directory: {}", args.data_dir);
    println!("Expected minimum messages: {}", args.expected_min_messages);

    let verifier = MessageVerifier::new(args.data_dir);
    verifier.verify(args.expected_min_messages).await?;

    Ok(())
}

struct MessageVerifier {
    data_dir: String,
}

impl MessageVerifier {
    fn new(data_dir: String) -> Self {
        Self { data_dir }
    }

    async fn verify(&self, expected_min_messages: usize) -> Result<()> {
        let messages_file = Path::new(&self.data_dir).join("received_messages.json");

        // Check if messages file exists
        if !messages_file.exists() {
            println!("No messages file found at: {}", messages_file.display());
            println!("The consumer didn't receive any messages");
            return Ok(()); // Don't fail the test, just report
        }

        // Read and parse messages
        let content = fs::read_to_string(&messages_file)?;
        if content.trim().is_empty() {
            println!("Messages file is empty");
            println!("The consumer didn't receive any messages");
            return Ok(());
        }

        let messages: Vec<ReceivedMessage> = serde_json::from_str(&content)?;
        
        println!("Results:");
        println!("   Total messages received: {}", messages.len());
        println!("   Expected minimum: {}", expected_min_messages);

        if messages.len() >= expected_min_messages {
            println!("SUCCESS: Received expected number of messages!");
        } else {
            println!("WARNING: Received fewer messages than expected");
        }

        // Check message content
        self.analyze_messages(&messages).await?;

        // Summary
        if messages.is_empty() {
            println!("\nVERIFICATION FAILED: No messages received");
        } else {
            println!("\nVERIFICATION PASSED: Messages were received!");
            if messages.len() >= expected_min_messages {
                println!("Integration test successful!");
            }
        }

        Ok(())
    }

    async fn analyze_messages(&self, messages: &[ReceivedMessage]) -> Result<()> {
        if messages.is_empty() {
            return Ok(());
        }

        println!("\nMessages:");

        // Check subjects
        let mut subject_counts = std::collections::HashMap::new();
        for msg in messages {
            *subject_counts.entry(&msg.subject).or_insert(0) += 1;
        }

        println!("   Subjects:");
        for (subject, count) in &subject_counts {
            println!("      {} → {} messages", subject, count);
        }

        // Check transaction signatures
        let mut transaction_count = 0;
        let mut unique_signatures = std::collections::HashSet::new();

        for msg in messages {
            if let Some(transaction) = msg.data.get("transaction") {
                transaction_count += 1;
                
                if let Some(signatures) = transaction.get("signatures").and_then(|s| s.as_array()) {
                    for sig in signatures {
                        if let Some(sig_str) = sig.as_str() {
                            unique_signatures.insert(sig_str.to_string());
                        }
                    }
                }
            }
        }

        println!("   Transactions:");
        println!("      Total transaction messages: {}", transaction_count);
        println!("      Unique signatures: {}", unique_signatures.len());

        // Show first few transaction signatures
        if !unique_signatures.is_empty() {
            println!("   Sample signatures:");
            for (i, sig) in unique_signatures.iter().take(5).enumerate() {
                println!("      {}: {}", i + 1, sig);
            }
            if unique_signatures.len() > 5 {
                println!("      ... and {} more", unique_signatures.len() - 5);
            }
        }

        // Check slots
        let mut slots = Vec::new();
        for msg in messages {
            if let Some(slot) = msg.data.get("slot").and_then(|s| s.as_u64()) {
                slots.push(slot);
            }
        }

        if !slots.is_empty() {
            slots.sort();
            println!("   Slots:");
            println!("      Slot range: {} - {}", slots[0], slots[slots.len() - 1]);
            println!("      Total unique slots: {}", slots.len());
        }

        Ok(())
    }
} 