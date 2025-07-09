use anyhow::Result;
use clap::Parser;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    message::Message,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};

use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;
use tracing::warn;

#[derive(Parser, Debug)]
#[command(name = "transaction-submitter")]
#[command(about = "Submit test transactions to Solana validator")]
struct Args {
    #[arg(long, default_value = "http://plugin-validator:8899")]
    solana_url: String,

    #[arg(long, default_value = "3")]
    num_transactions: u32,

    #[arg(long, default_value = "2")]
    sleep_between_tx: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    println!("Solana Transaction Submitter");
    println!("================================");
    println!("Validator URL: {}", args.solana_url);
    println!("Number of transactions: {}", args.num_transactions);
    println!("Sleep between transactions: {}s", args.sleep_between_tx);

    let submitter = TransactionSubmitter::new(args.solana_url)?;
    submitter.run(args.num_transactions, args.sleep_between_tx).await?;

    Ok(())
}

struct TransactionSubmitter {
    client: RpcClient,
    payer: Keypair,
    recipient: Keypair,
}

impl TransactionSubmitter {
    fn new(solana_url: String) -> Result<Self> {
        let client = RpcClient::new_with_commitment(solana_url, CommitmentConfig::confirmed());
        
        // Generate keypairs
        let payer = Keypair::new();
        let recipient = Keypair::new();

        Ok(Self {
            client,
            payer,
            recipient,
        })
    }

    async fn run(&self, num_transactions: u32, sleep_between_tx: u64) -> Result<()> {
        println!("Payer: {}", self.payer.pubkey());
        println!("Recipient: {}", self.recipient.pubkey());

        // Request airdrop
        println!("Requesting airdrop...");
        let airdrop_amount = 10_000_000_000; // 10 SOL
        
        match self.client.request_airdrop(&self.payer.pubkey(), airdrop_amount) {
            Ok(signature) => {
                println!("Airdrop signature: {}", signature);
                
                // Wait for airdrop confirmation
                println!("Waiting for airdrop confirmation...");
                self.wait_for_confirmation(&signature.to_string()).await?;
                
                // Check balance
                let balance = self.client.get_balance(&self.payer.pubkey())?;
                println!("Payer balance: {:.2} SOL", balance as f64 / 1_000_000_000.0);
            }
            Err(e) => {
                warn!("Airdrop failed: {}. Continuing anyway...", e);
            }
        }

        println!("Submitting {} transactions...", num_transactions);

        for i in 0..num_transactions {
            match self.create_and_submit_transaction(i + 1).await {
                Ok(signature) => {
                    println!("Transaction {} submitted: {}", i + 1, signature);
                    
                    // Wait for confirmation
                    match self.wait_for_confirmation(&signature.to_string()).await {
                        Ok(()) => println!("Transaction {} confirmed!", i + 1),
                        Err(e) => println!("Transaction {} confirmation failed: {}", i + 1, e),
                    }
                }
                Err(e) => {
                    println!("Failed to create/submit transaction {}: {}", i + 1, e);
                }
            }

            if i < num_transactions - 1 {
                println!("Sleeping {}s before next transaction...", sleep_between_tx);
                sleep(Duration::from_secs(sleep_between_tx)).await;
            }
        }

        println!("Transaction submission complete!");
        Ok(())
    }

    async fn create_and_submit_transaction(&self, tx_number: u32) -> Result<String> {
        // Get recent blockhash
        let recent_blockhash = self.client.get_latest_blockhash()?;

        // Create transfer amount (0.001 SOL)
        let lamports = 1_000_000;

        // Create transfer instruction
        let transfer_instruction = system_instruction::transfer(
            &self.payer.pubkey(),
            &self.recipient.pubkey(),
            lamports,
        );

        // Create memo instruction
        let memo_data = format!("Test transaction {} at {}", tx_number, self.get_timestamp());
        let memo_instruction = self.create_memo_instruction(&memo_data)?;

        // Create compute budget instruction to ensure the transaction gets processed
        let compute_budget_instruction = ComputeBudgetInstruction::set_compute_unit_limit(200_000);

        // Create transaction
        let message = Message::new(
            &[compute_budget_instruction, transfer_instruction, memo_instruction],
            Some(&self.payer.pubkey()),
        );

        let mut transaction = Transaction::new_unsigned(message);
        transaction.sign(&[&self.payer], recent_blockhash);

        // Submit transaction
        let signature = self.client.send_transaction(&transaction)?;
        Ok(signature.to_string())
    }

    fn create_memo_instruction(&self, memo_data: &str) -> Result<Instruction> {
        // Memo program ID
        let memo_program_id = Pubkey::from_str("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr")?;

        Ok(Instruction::new_with_bytes(
            memo_program_id,
            memo_data.as_bytes(),
            vec![],
        ))
    }

    async fn wait_for_confirmation(&self, signature_str: &str) -> Result<()> {
        let signature = solana_sdk::signature::Signature::from_str(signature_str)?;
        
        // Wait up to 30 seconds for confirmation
        for _ in 0..30 {
            match self.client.get_signature_status(&signature)? {
                Some(result) => {
                    match result {
                        Ok(()) => return Ok(()),
                        Err(e) => return Err(anyhow::anyhow!("Transaction failed: {:?}", e)),
                    }
                }
                None => {
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }

        Err(anyhow::anyhow!("Transaction confirmation timeout"))
    }

    fn get_timestamp(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
} 