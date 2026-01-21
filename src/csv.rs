use crate::transaction::{ClientTransaction, MoneyTransaction, Transaction};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::error::Error;
use std::path::Path;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
struct CsvRecord {
    #[serde(rename = "type")]
    tx_type: String,
    client: u16,
    tx: u32,
    amount: Option<Decimal>,
}

impl CsvRecord {
    fn into_transaction(self) -> Result<Transaction, String> {
        let tx_type = self.tx_type.trim();

        match tx_type {
            "deposit" => {
                let amount = self.amount.ok_or("Deposit requires an amount")?;
                let money_tx = MoneyTransaction::new(self.client, self.tx, amount)?;
                Ok(Transaction::Deposit(money_tx))
            }
            "withdrawal" => {
                let amount = self.amount.ok_or("Withdrawal requires an amount")?;
                let money_tx = MoneyTransaction::new(self.client, self.tx, amount)?;
                Ok(Transaction::Withdrawal(money_tx))
            }
            "dispute" => Ok(Transaction::Dispute(ClientTransaction::new(
                self.client,
                self.tx,
            ))),
            "resolve" => Ok(Transaction::Resolve(ClientTransaction::new(
                self.client,
                self.tx,
            ))),
            "chargeback" => Ok(Transaction::Chargeback(ClientTransaction::new(
                self.client,
                self.tx,
            ))),
            _ => Err(format!("Unknown transaction type: {}", tx_type)),
        }
    }
}

pub async fn process_csv_with_channel<P: AsRef<Path>>(
    path: P,
    tx: mpsc::Sender<Transaction>,
    cancel_token: CancellationToken,
) -> Result<(), Box<dyn Error>> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)?;

    for result in reader.deserialize::<CsvRecord>() {
        if cancel_token.is_cancelled() {
            println!("CSV processing cancelled");
            return Ok(());
        }

        let record = result?;
        let transaction = record.into_transaction()?;

        tokio::select! {
            _ = cancel_token.cancelled() => {
                println!("CSV processing cancelled");
                return Ok(());
            }
            result = tx.send(transaction) => {
                result?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_deposit_transaction() {
        let record = CsvRecord {
            tx_type: "deposit".to_string(),
            client: 1,
            tx: 100,
            amount: Some(dec!(50.00)),
        };

        let result = record.into_transaction();
        assert!(result.is_ok());

        let transaction = result.unwrap();
        assert_eq!(transaction.client_id(), 1);

        if let Transaction::Deposit(money_tx) = transaction {
            assert_eq!(money_tx.id.tx, 100);
            assert_eq!(money_tx.amount, dec!(50.00));
        } else {
            panic!("Expected Deposit transaction");
        }
    }

    #[test]
    fn test_withdrawal_transaction() {
        let record = CsvRecord {
            tx_type: "withdrawal".to_string(),
            client: 2,
            tx: 200,
            amount: Some(dec!(25.50)),
        };

        let result = record.into_transaction();
        assert!(result.is_ok());

        let transaction = result.unwrap();
        assert_eq!(transaction.client_id(), 2);

        if let Transaction::Withdrawal(money_tx) = transaction {
            assert_eq!(money_tx.id.tx, 200);
            assert_eq!(money_tx.amount, dec!(25.50));
        } else {
            panic!("Expected Withdrawal transaction");
        }
    }

    #[test]
    fn test_dispute_transaction() {
        let record = CsvRecord {
            tx_type: "dispute".to_string(),
            client: 3,
            tx: 300,
            amount: None,
        };

        let result = record.into_transaction();
        assert!(result.is_ok());

        let transaction = result.unwrap();
        if let Transaction::Dispute(client_tx) = transaction {
            assert_eq!(client_tx.client, 3);
            assert_eq!(client_tx.tx, 300);
        } else {
            panic!("Expected Dispute transaction");
        }
    }

    #[test]
    fn test_resolve_transaction() {
        let record = CsvRecord {
            tx_type: "resolve".to_string(),
            client: 4,
            tx: 400,
            amount: None,
        };

        let result = record.into_transaction();
        assert!(result.is_ok());

        let transaction = result.unwrap();
        if let Transaction::Resolve(client_tx) = transaction {
            assert_eq!(client_tx.client, 4);
            assert_eq!(client_tx.tx, 400);
        } else {
            panic!("Expected Resolve transaction");
        }
    }

    #[test]
    fn test_chargeback_transaction() {
        let record = CsvRecord {
            tx_type: "chargeback".to_string(),
            client: 5,
            tx: 500,
            amount: None,
        };

        let result = record.into_transaction();
        assert!(result.is_ok());

        let transaction = result.unwrap();
        if let Transaction::Chargeback(client_tx) = transaction {
            assert_eq!(client_tx.client, 5);
            assert_eq!(client_tx.tx, 500);
        } else {
            panic!("Expected Chargeback transaction");
        }
    }

    #[test]
    fn test_transaction_type_with_whitespace() {
        let record = CsvRecord {
            tx_type: "  deposit  ".to_string(),
            client: 1,
            tx: 100,
            amount: Some(dec!(10.00)),
        };

        let result = record.into_transaction();
        assert!(result.is_ok());
    }

    #[test]
    fn test_deposit_missing_amount() {
        let record = CsvRecord {
            tx_type: "deposit".to_string(),
            client: 1,
            tx: 100,
            amount: None,
        };

        let result = record.into_transaction();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Deposit requires an amount");
    }

    #[test]
    fn test_withdrawal_missing_amount() {
        let record = CsvRecord {
            tx_type: "withdrawal".to_string(),
            client: 1,
            tx: 100,
            amount: None,
        };

        let result = record.into_transaction();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Withdrawal requires an amount");
    }

    #[test]
    fn test_unknown_transaction_type() {
        let record = CsvRecord {
            tx_type: "unknown".to_string(),
            client: 1,
            tx: 100,
            amount: Some(dec!(10.00)),
        };

        let result = record.into_transaction();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown transaction type"));
    }
}
