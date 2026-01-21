use crate::transaction::Transaction;
use rust_decimal::Decimal;
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    sync::Arc,
};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct Ledger {
    transactions: HashMap<u32, Transaction>,
}

impl Ledger {
    pub fn new() -> Self {
        Self {
            transactions: HashMap::new(),
        }
    }

    pub fn add_transaction(&mut self, tx_id: u32, transaction: Transaction) {
        self.transactions.insert(tx_id, transaction);
    }

    pub fn get_transaction(&self, tx_id: u32) -> Option<&Transaction> {
        self.transactions.get(&tx_id)
    }

    pub fn get_transaction_mut(&mut self, tx_id: u32) -> Option<&mut Transaction> {
        self.transactions.get_mut(&tx_id)
    }

    pub fn is_disputed(&self, tx_id: u32) -> bool {
        if let Some(tx) = self.transactions.get(&tx_id) {
            match tx {
                Transaction::Deposit(money_tx) | Transaction::Withdrawal(money_tx) => {
                    money_tx.is_disputed()
                }
                _ => false,
            }
        } else {
            false
        }
    }

    pub fn is_chargedback(&self, tx_id: u32) -> bool {
        if let Some(tx) = self.transactions.get(&tx_id) {
            match tx {
                Transaction::Deposit(money_tx) | Transaction::Withdrawal(money_tx) => {
                    money_tx.is_chargedback()
                }
                _ => false,
            }
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct Account {
    pub client: u16,
    pub ledger: Ledger,
    pub available: Decimal,
    pub held: Decimal,
    pub total: Decimal,
    pub locked: bool,
}

impl Account {
    pub fn new(client: u16) -> Self {
        Self {
            client,
            ledger: Ledger::new(),
            available: Decimal::ZERO,
            held: Decimal::ZERO,
            total: Decimal::ZERO,
            locked: false,
        }
    }

    pub fn process_transaction(&mut self, transaction: Transaction) -> Result<(), Box<dyn Error>> {
        if self.locked && !matches!(transaction, Transaction::Chargeback(_)) {
            return Err("Account is locked".into());
        }

        match transaction {
            Transaction::Deposit(money_tx) => {
                // Check if transaction ID already exists
                if self.ledger.get_transaction(money_tx.id.tx).is_some() {
                    return Err(format!("Transaction ID {} already exists", money_tx.id.tx).into());
                }

                self.deposit(money_tx.amount);
                self.ledger
                    .add_transaction(money_tx.id.tx, Transaction::Deposit(money_tx));
                Ok(())
            }
            Transaction::Withdrawal(money_tx) => {
                // Check if transaction ID already exists
                if self.ledger.get_transaction(money_tx.id.tx).is_some() {
                    return Err(format!("Transaction ID {} already exists", money_tx.id.tx).into());
                }

                self.withdraw(money_tx.amount)?;
                self.ledger
                    .add_transaction(money_tx.id.tx, Transaction::Withdrawal(money_tx));
                Ok(())
            }
            Transaction::Dispute(client_tx) => {
                // Check if transaction is already disputed or chargedback
                if self.ledger.is_disputed(client_tx.tx) {
                    return Err("Transaction is already under dispute".into());
                }
                if self.ledger.is_chargedback(client_tx.tx) {
                    return Err("Cannot dispute a chargedback transaction".into());
                }

                // Get mutable reference to the transaction and mark it as disputed
                let amount = if let Some(tx) = self.ledger.get_transaction_mut(client_tx.tx) {
                    match tx {
                        Transaction::Deposit(money_tx) | Transaction::Withdrawal(money_tx) => {
                            money_tx.mark_disputed()?;
                            money_tx.amount
                        }
                        _ => return Err("Cannot dispute non-money transaction".into()),
                    }
                } else {
                    return Err("Transaction not found".into());
                };

                self.dispute(amount);
                Ok(())
            }
            Transaction::Resolve(client_tx) => {
                // Check if transaction is actually disputed
                if !self.ledger.is_disputed(client_tx.tx) {
                    return Err("Transaction is not under dispute".into());
                }

                // Get mutable reference to the transaction and resolve it
                let amount = if let Some(tx) = self.ledger.get_transaction_mut(client_tx.tx) {
                    match tx {
                        Transaction::Deposit(money_tx) | Transaction::Withdrawal(money_tx) => {
                            money_tx.resolve_dispute()?;
                            money_tx.amount
                        }
                        _ => return Err("Cannot resolve non-money transaction".into()),
                    }
                } else {
                    return Err("Transaction not found".into());
                };

                self.resolve(amount);
                Ok(())
            }
            Transaction::Chargeback(client_tx) => {
                // Check if transaction is actually disputed
                if !self.ledger.is_disputed(client_tx.tx) {
                    return Err("Transaction is not under dispute".into());
                }

                // Get mutable reference to the transaction and mark it as chargedback
                let amount = if let Some(tx) = self.ledger.get_transaction_mut(client_tx.tx) {
                    match tx {
                        Transaction::Deposit(money_tx) | Transaction::Withdrawal(money_tx) => {
                            money_tx.mark_chargedback()?;
                            money_tx.amount
                        }
                        _ => return Err("Cannot chargeback non-money transaction".into()),
                    }
                } else {
                    return Err("Transaction not found".into());
                };

                self.chargeback(amount);
                Ok(())
            }
        }
    }

    pub fn deposit(&mut self, amount: Decimal) {
        if !self.locked {
            self.available += amount;
            self.total += amount;
        }
    }

    pub fn withdraw(&mut self, amount: Decimal) -> Result<(), String> {
        if self.locked {
            return Err("Account is locked".to_string());
        }

        if self.available >= amount {
            self.available -= amount;
            self.total -= amount;
            Ok(())
        } else {
            Err("Insufficient funds".to_string())
        }
    }

    pub fn dispute(&mut self, amount: Decimal) {
        if !self.locked {
            self.available -= amount;
            self.held += amount;
        }
    }

    pub fn resolve(&mut self, amount: Decimal) {
        if !self.locked {
            self.held -= amount;
            self.available += amount;
        }
    }

    pub fn chargeback(&mut self, amount: Decimal) {
        self.held -= amount;
        self.total -= amount;
        self.locked = true;
    }
}

#[derive(Debug, Clone)]
pub struct AccountManager {
    accounts: Arc<RwLock<HashMap<u16, Account>>>,
}

impl AccountManager {
    pub fn new() -> Self {
        Self {
            accounts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn process_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<(), Box<dyn Error>> {
        let client_id = transaction.client_id();

        let mut accounts = self.accounts.write().await;
        let account = accounts
            .entry(client_id)
            .or_insert_with(|| Account::new(client_id));
        account.process_transaction(transaction)
    }

    #[cfg(test)]
    pub async fn get_account(&self, client: u16) -> Option<Account> {
        let accounts = self.accounts.read().await;
        accounts.get(&client).cloned()
    }

    pub async fn accounts(&self) -> HashMap<u16, Account> {
        let accounts = self.accounts.read().await;
        accounts.clone()
    }

    #[cfg(test)]
    pub async fn total_accounts(&self) -> usize {
        let accounts = self.accounts.read().await;
        accounts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_account_creation() {
        let account = Account::new(1);
        assert_eq!(account.client, 1);
        assert_eq!(account.available, Decimal::ZERO);
        assert_eq!(account.held, Decimal::ZERO);
        assert_eq!(account.total, Decimal::ZERO);
        assert_eq!(account.locked, false);
    }

    #[test]
    fn test_deposit() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.00));
        assert_eq!(account.available, dec!(100.00));
        assert_eq!(account.total, dec!(100.00));
    }

    #[test]
    fn test_withdraw() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.00));

        let result = account.withdraw(dec!(50.00));
        assert!(result.is_ok());
        assert_eq!(account.available, dec!(50.00));
        assert_eq!(account.total, dec!(50.00));
    }

    #[test]
    fn test_withdraw_insufficient_funds() {
        let mut account = Account::new(1);
        account.deposit(dec!(50.00));

        let result = account.withdraw(dec!(100.00));
        assert!(result.is_err());
        assert_eq!(account.available, dec!(50.00));
    }

    #[test]
    fn test_dispute() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.00));
        account.dispute(dec!(30.00));

        assert_eq!(account.available, dec!(70.00));
        assert_eq!(account.held, dec!(30.00));
        assert_eq!(account.total, dec!(100.00));
    }

    #[test]
    fn test_resolve() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.00));
        account.dispute(dec!(30.00));
        account.resolve(dec!(30.00));

        assert_eq!(account.available, dec!(100.00));
        assert_eq!(account.held, Decimal::ZERO);
        assert_eq!(account.total, dec!(100.00));
    }

    #[test]
    fn test_chargeback() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.00));
        account.dispute(dec!(30.00));
        account.chargeback(dec!(30.00));

        assert_eq!(account.available, dec!(70.00));
        assert_eq!(account.held, Decimal::ZERO);
        assert_eq!(account.total, dec!(70.00));
        assert_eq!(account.locked, true);
    }

    #[tokio::test]
    async fn test_account_manager() {
        use crate::transaction::{MoneyTransaction, Transaction};

        let manager = AccountManager::new();

        let deposit1 = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        manager.process_transaction(deposit1).await.unwrap();

        let deposit2 = Transaction::Deposit(MoneyTransaction::new(2, 2, dec!(200.00)).unwrap());
        manager.process_transaction(deposit2).await.unwrap();

        assert_eq!(manager.total_accounts().await, 2);
        assert_eq!(
            manager.get_account(1).await.unwrap().available,
            dec!(100.00)
        );
        assert_eq!(
            manager.get_account(2).await.unwrap().available,
            dec!(200.00)
        );
    }

    #[test]
    fn test_resolve_without_dispute() {
        use crate::transaction::{ClientTransaction, MoneyTransaction, Transaction};

        let mut account = Account::new(1);

        // Add a deposit
        let deposit = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        account.process_transaction(deposit).unwrap();

        // Try to resolve without disputing first - should fail
        let resolve = Transaction::Resolve(ClientTransaction::new(1, 1));
        let result = account.process_transaction(resolve);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Transaction is not under dispute"
        );
    }

    #[test]
    fn test_chargeback_without_dispute() {
        use crate::transaction::{ClientTransaction, MoneyTransaction, Transaction};

        let mut account = Account::new(1);

        // Add a deposit
        let deposit = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        account.process_transaction(deposit).unwrap();

        // Try to chargeback without disputing first - should fail
        let chargeback = Transaction::Chargeback(ClientTransaction::new(1, 1));
        let result = account.process_transaction(chargeback);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Transaction is not under dispute"
        );
    }

    #[test]
    fn test_duplicate_dispute() {
        use crate::transaction::{ClientTransaction, MoneyTransaction, Transaction};

        let mut account = Account::new(1);

        // Add a deposit
        let deposit = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        account.process_transaction(deposit).unwrap();

        // First dispute - should succeed
        let dispute1 = Transaction::Dispute(ClientTransaction::new(1, 1));
        let result1 = account.process_transaction(dispute1);
        assert!(result1.is_ok());

        // Second dispute on same transaction - should fail
        let dispute2 = Transaction::Dispute(ClientTransaction::new(1, 1));
        let result2 = account.process_transaction(dispute2);

        assert!(result2.is_err());
        assert_eq!(
            result2.unwrap_err().to_string(),
            "Transaction is already under dispute"
        );
    }

    #[test]
    fn test_dispute_resolve_cycle() {
        use crate::transaction::{ClientTransaction, MoneyTransaction, Transaction};

        let mut account = Account::new(1);

        // Add a deposit
        let deposit = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        account.process_transaction(deposit).unwrap();
        assert_eq!(account.available, dec!(100.00));
        assert_eq!(account.held, dec!(0.00));

        // Dispute the transaction
        let dispute = Transaction::Dispute(ClientTransaction::new(1, 1));
        account.process_transaction(dispute).unwrap();
        assert_eq!(account.available, dec!(0.00));
        assert_eq!(account.held, dec!(100.00));
        assert!(account.ledger.is_disputed(1));

        // Resolve the dispute
        let resolve = Transaction::Resolve(ClientTransaction::new(1, 1));
        account.process_transaction(resolve).unwrap();
        assert_eq!(account.available, dec!(100.00));
        assert_eq!(account.held, dec!(0.00));
        assert!(!account.ledger.is_disputed(1));
    }

    #[test]
    fn test_dispute_chargeback_cycle() {
        use crate::transaction::{ClientTransaction, MoneyTransaction, Transaction};

        let mut account = Account::new(1);

        // Add a deposit
        let deposit = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        account.process_transaction(deposit).unwrap();
        assert_eq!(account.available, dec!(100.00));
        assert_eq!(account.held, dec!(0.00));
        assert_eq!(account.total, dec!(100.00));

        // Dispute the transaction
        let dispute = Transaction::Dispute(ClientTransaction::new(1, 1));
        account.process_transaction(dispute).unwrap();
        assert_eq!(account.available, dec!(0.00));
        assert_eq!(account.held, dec!(100.00));
        assert_eq!(account.total, dec!(100.00));
        assert!(account.ledger.is_disputed(1));

        // Chargeback the dispute
        let chargeback = Transaction::Chargeback(ClientTransaction::new(1, 1));
        account.process_transaction(chargeback).unwrap();
        assert_eq!(account.available, dec!(0.00));
        assert_eq!(account.held, dec!(0.00));
        assert_eq!(account.total, dec!(0.00));
        assert_eq!(account.locked, true);
        assert!(!account.ledger.is_disputed(1));
    }

    #[test]
    fn test_duplicate_transaction_id_deposit() {
        use crate::transaction::{MoneyTransaction, Transaction};

        let mut account = Account::new(1);

        // First deposit with tx ID 1
        let deposit1 = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        let result1 = account.process_transaction(deposit1);
        assert!(result1.is_ok());
        assert_eq!(account.available, dec!(100.00));

        // Try another deposit with same tx ID 1 - should fail
        let deposit2 = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(50.00)).unwrap());
        let result2 = account.process_transaction(deposit2);

        assert!(result2.is_err());
        assert_eq!(
            result2.unwrap_err().to_string(),
            "Transaction ID 1 already exists"
        );

        // Balance should remain unchanged
        assert_eq!(account.available, dec!(100.00));
    }

    #[test]
    fn test_duplicate_transaction_id_withdrawal() {
        use crate::transaction::{MoneyTransaction, Transaction};

        let mut account = Account::new(1);

        // Deposit to have funds
        let deposit = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        account.process_transaction(deposit).unwrap();

        // First withdrawal with tx ID 2
        let withdrawal1 =
            Transaction::Withdrawal(MoneyTransaction::new(1, 2, dec!(30.00)).unwrap());
        let result1 = account.process_transaction(withdrawal1);
        assert!(result1.is_ok());
        assert_eq!(account.available, dec!(70.00));

        // Try another withdrawal with same tx ID 2 - should fail
        let withdrawal2 =
            Transaction::Withdrawal(MoneyTransaction::new(1, 2, dec!(20.00)).unwrap());
        let result2 = account.process_transaction(withdrawal2);

        assert!(result2.is_err());
        assert_eq!(
            result2.unwrap_err().to_string(),
            "Transaction ID 2 already exists"
        );

        // Balance should remain unchanged
        assert_eq!(account.available, dec!(70.00));
    }

    #[test]
    fn test_duplicate_transaction_id_mixed() {
        use crate::transaction::{MoneyTransaction, Transaction};

        let mut account = Account::new(1);

        // Deposit with tx ID 1
        let deposit = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        account.process_transaction(deposit).unwrap();

        // Try withdrawal with same tx ID 1 - should fail
        let withdrawal = Transaction::Withdrawal(MoneyTransaction::new(1, 1, dec!(50.00)).unwrap());
        let result = account.process_transaction(withdrawal);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Transaction ID 1 already exists"
        );

        // Balance should remain unchanged
        assert_eq!(account.available, dec!(100.00));
    }

    #[test]
    fn test_chargedback_transactions_are_marked() {
        use crate::transaction::{ClientTransaction, MoneyTransaction, Transaction};

        let mut account = Account::new(1);

        // Add a deposit
        let deposit = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        account.process_transaction(deposit).unwrap();
        
        // Transaction should not be chargedback initially
        assert!(!account.ledger.is_chargedback(1));

        // Dispute the transaction
        let dispute = Transaction::Dispute(ClientTransaction::new(1, 1));
        account.process_transaction(dispute).unwrap();
        assert!(!account.ledger.is_chargedback(1));

        let chargeback = Transaction::Chargeback(ClientTransaction::new(1, 1));
        account.process_transaction(chargeback).unwrap();
        
        // Transaction should now be marked as chargedback
        assert!(account.ledger.is_chargedback(1));
        assert_eq!(account.locked, true);
    }

    #[test]
    fn test_multiple_chargebacks_after_account_locked() {
        use crate::transaction::{ClientTransaction, MoneyTransaction, Transaction};

        let mut account = Account::new(1);

        // Add multiple deposits
        let deposit1 = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        account.process_transaction(deposit1).unwrap();
        
        let deposit2 = Transaction::Deposit(MoneyTransaction::new(1, 2, dec!(50.00)).unwrap());
        account.process_transaction(deposit2).unwrap();
        
        let deposit3 = Transaction::Deposit(MoneyTransaction::new(1, 3, dec!(75.00)).unwrap());
        account.process_transaction(deposit3).unwrap();
        
        assert_eq!(account.available, dec!(225.00));
        assert_eq!(account.total, dec!(225.00));

        // Dispute all three transactions
        let dispute1 = Transaction::Dispute(ClientTransaction::new(1, 1));
        account.process_transaction(dispute1).unwrap();
        
        let dispute2 = Transaction::Dispute(ClientTransaction::new(1, 2));
        account.process_transaction(dispute2).unwrap();
        
        let dispute3 = Transaction::Dispute(ClientTransaction::new(1, 3));
        account.process_transaction(dispute3).unwrap();
        
        assert_eq!(account.available, dec!(0.00));
        assert_eq!(account.held, dec!(225.00));
        assert_eq!(account.total, dec!(225.00));
        assert_eq!(account.locked, false);

        // First chargeback - locks the account
        let chargeback1 = Transaction::Chargeback(ClientTransaction::new(1, 1));
        account.process_transaction(chargeback1).unwrap();
        
        assert_eq!(account.available, dec!(0.00));
        assert_eq!(account.held, dec!(125.00)); // 225 - 100
        assert_eq!(account.total, dec!(125.00)); // 225 - 100
        assert_eq!(account.locked, true);
        assert!(account.ledger.is_chargedback(1));

        // Second chargeback - should still work even though account is locked
        let chargeback2 = Transaction::Chargeback(ClientTransaction::new(1, 2));
        let result2 = account.process_transaction(chargeback2);
        assert!(result2.is_ok(), "Second chargeback should succeed on locked account");
        
        assert_eq!(account.available, dec!(0.00));
        assert_eq!(account.held, dec!(75.00)); // 125 - 50
        assert_eq!(account.total, dec!(75.00)); // 125 - 50
        assert_eq!(account.locked, true);
        assert!(account.ledger.is_chargedback(2));

        // Third chargeback - should also work
        let chargeback3 = Transaction::Chargeback(ClientTransaction::new(1, 3));
        let result3 = account.process_transaction(chargeback3);
        assert!(result3.is_ok(), "Third chargeback should succeed on locked account");
        
        assert_eq!(account.available, dec!(0.00));
        assert_eq!(account.held, dec!(0.00)); // 75 - 75
        assert_eq!(account.total, dec!(0.00)); // 75 - 75
        assert_eq!(account.locked, true);
        assert!(account.ledger.is_chargedback(3));
    }

    #[test]
    fn test_locked_account_rejects_non_chargeback_transactions() {
        use crate::transaction::{ClientTransaction, MoneyTransaction, Transaction};

        let mut account = Account::new(1);

        // Setup: Create and chargeback a transaction to lock the account
        let deposit1 = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        account.process_transaction(deposit1).unwrap();
        
        let dispute1 = Transaction::Dispute(ClientTransaction::new(1, 1));
        account.process_transaction(dispute1).unwrap();
        
        let chargeback1 = Transaction::Chargeback(ClientTransaction::new(1, 1));
        account.process_transaction(chargeback1).unwrap();
        
        assert_eq!(account.locked, true);

        // Try to process a new deposit - should fail
        let deposit2 = Transaction::Deposit(MoneyTransaction::new(1, 2, dec!(50.00)).unwrap());
        let result = account.process_transaction(deposit2);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Account is locked");

        // Try to process a withdrawal - should fail
        let withdrawal = Transaction::Withdrawal(MoneyTransaction::new(1, 3, dec!(10.00)).unwrap());
        let result = account.process_transaction(withdrawal);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Account is locked");
    }

    #[test]
    fn test_ledger_is_disputed() {
        use crate::transaction::{ClientTransaction, MoneyTransaction, Transaction};

        let mut account = Account::new(1);

        // Add a deposit
        let deposit = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        account.process_transaction(deposit).unwrap();
        
        // Initially should not be disputed
        assert!(!account.ledger.is_disputed(1));
        
        // After disputing, should return true
        let dispute = Transaction::Dispute(ClientTransaction::new(1, 1));
        account.process_transaction(dispute).unwrap();
        assert!(account.ledger.is_disputed(1));
        
        // After resolving, should not be disputed
        let resolve = Transaction::Resolve(ClientTransaction::new(1, 1));
        account.process_transaction(resolve).unwrap();
        assert!(!account.ledger.is_disputed(1));
    }

    #[test]
    fn test_ledger_is_chargedback() {
        use crate::transaction::{ClientTransaction, MoneyTransaction, Transaction};

        let mut account = Account::new(1);

        // Add a deposit
        let deposit = Transaction::Deposit(MoneyTransaction::new(1, 1, dec!(100.00)).unwrap());
        account.process_transaction(deposit).unwrap();
        
        // Initially should not be chargedback
        assert!(!account.ledger.is_chargedback(1));
        
        // Dispute the transaction
        let dispute = Transaction::Dispute(ClientTransaction::new(1, 1));
        account.process_transaction(dispute).unwrap();
        assert!(!account.ledger.is_chargedback(1));
        
        // After chargeback, should return true
        let chargeback = Transaction::Chargeback(ClientTransaction::new(1, 1));
        account.process_transaction(chargeback).unwrap();
        assert!(account.ledger.is_chargedback(1));
    }

    #[test]
    fn test_ledger_is_disputed_and_is_chargedback_for_nonexistent_tx() {
        let account = Account::new(1);
        
        // For a non-existent transaction, both should return false
        assert!(!account.ledger.is_disputed(999));
        assert!(!account.ledger.is_chargedback(999));
    }
}
