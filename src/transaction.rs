use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientTransaction {
    pub client: u16,
    pub tx: u32,
}

impl ClientTransaction {
    pub fn new(client: u16, tx: u32) -> Self {
        Self { client, tx }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    Normal,
    Disputed,
    Chargedback,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoneyTransaction {
    pub id: ClientTransaction,
    pub amount: Decimal,
    pub timestamp: DateTime<Utc>,
    pub state: TransactionState,
}

impl MoneyTransaction {
    pub fn new(client: u16, tx: u32, amount: Decimal) -> Result<Self, String> {
        if amount <= Decimal::ZERO {
            return Err(format!(
                "Transaction amount must be positive, got: {}",
                amount
            ));
        }

        Ok(Self {
            id: ClientTransaction::new(client, tx),
            amount,
            timestamp: Utc::now(),
            state: TransactionState::Normal,
        })
    }

    pub fn is_disputed(&self) -> bool {
        self.state == TransactionState::Disputed
    }

    pub fn is_chargedback(&self) -> bool {
        self.state == TransactionState::Chargedback
    }

    pub fn mark_disputed(&mut self) -> Result<(), String> {
        if self.state == TransactionState::Disputed {
            return Err("Transaction is already under dispute".to_string());
        }
        if self.state == TransactionState::Chargedback {
            return Err("Cannot dispute a chargedback transaction".to_string());
        }
        self.state = TransactionState::Disputed;
        Ok(())
    }

    pub fn resolve_dispute(&mut self) -> Result<(), String> {
        if self.state != TransactionState::Disputed {
            return Err("Transaction is not under dispute".to_string());
        }
        self.state = TransactionState::Normal;
        Ok(())
    }

    pub fn mark_chargedback(&mut self) -> Result<(), String> {
        if self.state != TransactionState::Disputed {
            return Err("Transaction is not under dispute".to_string());
        }
        self.state = TransactionState::Chargedback;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Transaction {
    Deposit(MoneyTransaction),
    Withdrawal(MoneyTransaction),
    Dispute(ClientTransaction),
    Resolve(ClientTransaction),
    Chargeback(ClientTransaction),
}

impl Transaction {
    pub fn client_id(&self) -> u16 {
        match self {
            Transaction::Deposit(tx) | Transaction::Withdrawal(tx) => tx.id.client,
            Transaction::Dispute(id) | Transaction::Resolve(id) | Transaction::Chargeback(id) => {
                id.client
            }
        }
    }

    #[allow(dead_code)]
    pub fn transaction_id(&self) -> u32 {
        match self {
            Transaction::Deposit(tx) | Transaction::Withdrawal(tx) => tx.id.tx,
            Transaction::Dispute(id) | Transaction::Resolve(id) | Transaction::Chargeback(id) => {
                id.tx
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_money_transaction_creation() {
        let result = MoneyTransaction::new(1, 100, dec!(50.00));
        assert!(result.is_ok());

        let tx = result.unwrap();
        assert_eq!(tx.id.client, 1);
        assert_eq!(tx.id.tx, 100);
        assert_eq!(tx.amount, dec!(50.00));
    }

    #[test]
    fn test_money_transaction_validation() {
        let result = MoneyTransaction::new(1, 100, Decimal::ZERO);
        assert!(result.is_err());

        let result = MoneyTransaction::new(1, 100, dec!(-10.00));
        assert!(result.is_err());
    }

    #[test]
    fn test_transaction_client_id() {
        let tx = Transaction::Deposit(MoneyTransaction::new(5, 200, dec!(100.00)).unwrap());
        assert_eq!(tx.client_id(), 5);

        let dispute = Transaction::Dispute(ClientTransaction::new(10, 300));
        assert_eq!(dispute.client_id(), 10);
    }
}
