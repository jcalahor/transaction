mod account;
mod csv;
mod transaction;

use crate::account::AccountManager;
use crate::transaction::Transaction;
use flexi_logger::{Logger, WriteMode};
use log::{error, info};
use rust_decimal::Decimal;
use std::env;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Format decimal with at least 1 decimal place, up to 4 decimal places
fn format_decimal(value: Decimal) -> String {
    let s = value.to_string();

    // If it already has a decimal point, return as-is
    if s.contains('.') {
        return s;
    }

    // Otherwise, add .0
    format!("{}.0", s)
}

#[tokio::main]
async fn main() {
    // Initialize flexi_logger with BufferAndFlush for better performance
    let _logger_handle = Logger::try_with_str("info")
        .expect("Failed to create logger")
        .log_to_file(
            flexi_logger::FileSpec::default()
                .directory(".")
                .basename("session")
                .suffix("log")
                .suppress_timestamp(),
        )
        .write_mode(WriteMode::BufferAndFlush)
        .start()
        .expect("Failed to start logger");

    info!("Starting transaction processor");

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <csv_file>", args[0]);
        std::process::exit(1);
    }

    let filename = args[1].clone();
    info!("Processing file: {}", filename);

    let cancel_token = CancellationToken::new();
    let account_manager = Arc::new(AccountManager::new());

    let (tx, mut rx) = mpsc::channel::<Transaction>(100);

    let sender_cancel_token = cancel_token.clone();
    let sender_handle: tokio::task::JoinHandle<()> = tokio::spawn(async move {
        match csv::process_csv_with_channel(&filename, tx, sender_cancel_token).await {
            Ok(_) => eprintln!("Finished reading CSV"),
            Err(e) => eprintln!("Error processing CSV: {}", e),
        }
    });

    let manager_clone = Arc::clone(&account_manager);
    let receiver_handle = tokio::spawn(async move {
        while let Some(transaction) = rx.recv().await {
            info!("Received transaction: {:?}", transaction);
            match manager_clone.process_transaction(transaction).await {
                Ok(_) => {
                    info!("Transaction processed successfully");
                }
                Err(e) => {
                    error!("Error processing transaction: {}", e);
                    eprintln!("Error processing transaction: {}", e);
                }
            }
        }
    });

    tokio::select! {
        _ = signal::ctrl_c() => {
            eprintln!("\nReceived Ctrl-C, shutting down gracefully...");
            cancel_token.cancel();
        }
        result = async {
            tokio::try_join!(sender_handle, receiver_handle)
        } => {
            match result {
                Ok(_) => {
                    eprintln!("Finished reading CSV");

                    // Output CSV to stdout
                    println!("client, available, held, total, locked");

                    let accounts = account_manager.accounts().await;
                    let mut clients: Vec<_> = accounts.keys().collect();
                    clients.sort();

                    for client_id in clients {
                        if let Some(account) = accounts.get(client_id) {
                            println!("{}, {}, {}, {}, {}",
                                account.client,
                                format_decimal(account.available),
                                format_decimal(account.held),
                                format_decimal(account.total),
                                account.locked
                            );
                        }
                    }

                    eprintln!("Processing complete");
                },
                Err(e) => eprintln!("Error: {:?}", e),
            }
        }
    }

    // Flush logs before exiting
    _logger_handle.flush();
}
