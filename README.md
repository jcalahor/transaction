# Transaction Processing Engine

A robust, async transaction processing system built with Rust that handles deposits, withdrawals, disputes, resolves, and chargebacks with proper state management.

## Table of Contents
- [Overview](#overview)
- [Architecture](#architecture)
- [Running the Application](#running-the-application)
- [Testing](#testing)
- [Edge Cases & State Management](#edge-cases--state-management)
- [Data Format](#data-format)

## Overview

This transaction processing engine reads CSV files containing financial transactions and outputs account states. It supports:

- **Deposits & Withdrawals**: Basic money operations
- **Disputes**: Challenge transactions
- **Resolves**: Accept disputed transactions
- **Chargebacks**: Reverse disputed transactions and lock accounts
- **State Management**: Enforces valid transaction state transitions
- **Async Processing**: Built with Tokio for high performance
- **Precision**: Decimal arithmetic with up to 4 decimal places

## Architecture

### Data Flow Architecture

The application uses an asynchronous pipeline with Tokio channels for efficient transaction processing:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Transaction Processing Pipeline                  │
└─────────────────────────────────────────────────────────────────────────┘

    CSV Input File
         │
         │ (1) Read CSV rows
         ↓
  ┌──────────────────┐
  │  CSV Reader      │  (Async Task 1)
  │  process_csv_*   │  - Parses CSV rows
  │                  │  - Validates format
  └─────────┬────────┘  - Creates Transaction objects
            │
            │ (2) Send via Tokio MPSC Channel (capacity: 100)
            ↓
    ┌───────────────────┐
    │   MPSC Channel    │  Buffer: Transaction queue
    │   (tx → rx)       │  Provides backpressure
    └────────┬──────────┘
             │
             │ (3) Receive transactions one-by-one
             ↓
  ┌─────────────────────┐
  │ Transaction Receiver│  (Async Task 2)
  │ Loop                │  - Receives from channel
  │                     │  - Forwards to AccountManager
  └──────────┬──────────┘
             │
             │ (4) Process transaction
             ↓
  ┌─────────────────────┐
  │  AccountManager     │  Thread-safe (Arc<RwLock>)
  │  process_transaction│  - Locks account
  └──────────┬──────────┘  - Validates state
             │             - Updates balances
             │ (5) Mutate account state
             ↓
  ┌─────────────────────┐
  │   Account + Ledger  │  Per-client state
  │   - available       │  - Transaction history
  │   - held            │  - State management
  │   - total           │  - Balance tracking
  │   - locked          │
  └──────────┬──────────┘
             │
             │ (6) All transactions processed
             ↓
  ┌─────────────────────┐
  │  Output Generator   │  - Sort by client ID
  │                     │  - Format as CSV
  └──────────┬──────────┘  - Write to stdout
             │
             ↓
      CSV Output (stdout)
```

**Key Benefits:**
- **Async Processing**: Non-blocking I/O with Tokio
- **Backpressure**: Channel capacity prevents memory overflow
- **Concurrency Safety**: RwLock ensures thread-safe account access
- **Graceful Shutdown**: Ctrl-C handling with CancellationToken
- **Error Isolation**: Failed transactions don't stop processing

### Core Structures

#### `Transaction` (src/transaction.rs)
Enum representing all transaction types:
```rust
pub enum Transaction {
    Deposit(MoneyTransaction),      // Add funds
    Withdrawal(MoneyTransaction),   // Remove funds
    Dispute(ClientTransaction),     // Challenge a transaction
    Resolve(ClientTransaction),     // Accept disputed transaction
    Chargeback(ClientTransaction),  // Reverse disputed transaction
}
```

**MoneyTransaction**: Contains client ID, transaction ID, amount, timestamp, and state
**ClientTransaction**: Contains only client ID and transaction ID (for disputes/resolves/chargebacks)

#### `TransactionState` (src/transaction.rs)
Enum tracking the state of each transaction:
```rust
pub enum TransactionState {
    Normal,       // Transaction in normal state
    Disputed,     // Transaction is under dispute
    Chargedback,  // Transaction has been charged back
}
```

State is encapsulated within each `MoneyTransaction` for better cohesion and self-contained state management.

#### `TransactionError` (src/transaction.rs)
Type-safe error handling for transaction state operations:
```rust
pub enum TransactionError {
    AlreadyDisputed,      // Cannot dispute an already-disputed transaction
    NotDisputed,          // Cannot resolve/chargeback a non-disputed transaction
    AlreadyChargedback,   // Cannot dispute a chargedback transaction
    InvalidAmount(String),// Amount validation errors (reserved for future use)
}
```

#### `Account` (src/account.rs)
Represents a client account with:
```rust
pub struct Account {
    pub client: u16,              // Client identifier
    pub ledger: Ledger,           // Transaction history
    pub available: Decimal,       // Available funds
    pub held: Decimal,            // Funds held in dispute
    pub total: Decimal,           // Total = available + held
    pub locked: bool,             // Account locked after chargeback
}
```

#### `Ledger` (src/account.rs)
Transaction ledger storing all transactions:
```rust
pub struct Ledger {
    transactions: HashMap<u32, Transaction>,  // All transactions with their state
}
```

**Key Methods:**
- `get_transaction(tx_id)`: Retrieve transaction by ID
- `get_transaction_mut(tx_id)`: Get mutable reference to transaction
- `is_disputed(tx_id)`: Check if transaction is disputed (delegates to transaction state)
- `is_chargedback(tx_id)`: Check if transaction is chargedback (delegates to transaction state)
- `add_transaction(tx_id, tx)`: Add new transaction to ledger

State management is delegated to the `MoneyTransaction` itself, eliminating the need for separate tracking HashSets.

#### `AccountManager` (src/account.rs)
Thread-safe account manager using async RwLock:
```rust
pub struct AccountManager {
    accounts: Arc<RwLock<HashMap<u16, Account>>>,
}
```

## Running the Application

### Prerequisites
- Rust 1.70+ (uses 2024 edition)
- Cargo

### Build
```bash
cargo build --release
```

### Run
```bash
# Process CSV file and output to stdout
cargo run -- transactions.csv > accounts.csv

# Or use the compiled binary
./target/release/transactions input.csv > output.csv
```

### Input Format (CSV)
```csv
type, client, tx, amount
deposit, 1, 1, 1.5
deposit, 2, 2, 2.0
withdrawal, 1, 3, 0.5
dispute, 1, 1,
resolve, 1, 1,
```

### Output Format (CSV)
```csv
client, available, held, total, locked
1, 1.5, 0.0, 1.5, false
2, 2.0, 0.0, 2.0, false
```

### Features
- **Logging**: Buffered logging to `session.log` for debugging
- **Error Handling**: Continues processing on errors, logs issues
- **Graceful Shutdown**: Ctrl-C handling with proper cleanup
- **Decimal Formatting**: Displays at least 1 decimal place, up to 4

## Testing

### Run All Tests
```bash
cargo test
```

### Test Categories

#### Unit Tests (38 tests)
Unit tests verify individual components in isolation using direct function calls and assertions.

**Account Tests (28 tests) - `src/account.rs`:**

*Basic Operations:*
1. `test_account_creation`: Verifies new accounts start with zero balances and unlocked status
2. `test_deposit`: Validates deposit increases available and total balances
3. `test_withdraw`: Tests successful withdrawal reduces balances correctly
4. `test_withdraw_insufficient_funds`: Ensures withdrawals fail when funds are insufficient
5. `test_dispute`: Verifies dispute moves funds from available to held
6. `test_resolve`: Tests resolve moves funds from held back to available
7. `test_chargeback`: Confirms chargeback reduces total and locks account

*State Transition Tests:*
8. `test_dispute_resolve_cycle`: Full cycle - deposit → dispute → resolve
9. `test_dispute_chargeback_cycle`: Full cycle - deposit → dispute → chargeback
10. `test_resolve_without_dispute`: Ensures resolve fails if transaction not disputed
11. `test_chargeback_without_dispute`: Ensures chargeback fails if transaction not disputed
12. `test_duplicate_dispute`: Prevents disputing same transaction twice

*Transaction ID Uniqueness:*
13. `test_duplicate_transaction_id_deposit`: Two deposits with same ID should fail
14. `test_duplicate_transaction_id_withdrawal`: Two withdrawals with same ID should fail
15. `test_duplicate_transaction_id_mixed`: Deposit and withdrawal with same ID should fail

*Locked Account Behavior:*
16. `test_multiple_chargebacks_after_account_locked`: Verifies multiple chargebacks work after lock
17. `test_locked_account_rejects_non_chargeback_transactions`: Ensures locked accounts reject new operations

*Ledger State Queries:*
18. `test_ledger_is_disputed`: Tests Ledger.is_disputed() method accuracy
19. `test_ledger_is_chargedback`: Tests Ledger.is_chargedback() method accuracy
20. `test_ledger_is_disputed_and_is_chargedback_for_nonexistent_tx`: Returns false for non-existent transactions

*Advanced Cycles:*
21. `test_dispute_resolve_dispute_cycle`: Verifies transaction can be re-disputed after resolution
22. `test_chargedback_transactions_are_marked`: Confirms chargedback transactions are tracked
23. `test_account_manager`: Tests async AccountManager with multiple clients

**CSV Tests (4 tests) - `src/csv.rs`:**
1. `test_deposit_transaction`: Parses deposit with amount
2. `test_withdrawal_transaction`: Parses withdrawal with amount
3. `test_dispute_transaction`: Parses dispute (no amount required)
4. `test_chargeback_transaction`: Parses chargeback (no amount required)
5. `test_resolve_transaction`: Parses resolve (no amount required)
6. `test_unknown_transaction_type`: Handles invalid transaction types
7. `test_transaction_type_with_whitespace`: Trims whitespace from types
8. `test_deposit_missing_amount`: Rejects deposits without amounts
9. `test_withdrawal_missing_amount`: Rejects withdrawals without amounts

**Transaction Tests (6 tests) - `src/transaction.rs`:**
1. `test_money_transaction_creation`: Creates valid MoneyTransaction
2. `test_money_transaction_validation`: Rejects zero/negative amounts
3. `test_transaction_client_id`: Extracts client ID from all transaction types
4. `test_is_disputed`: Tests MoneyTransaction.is_disputed() through state changes
5. `test_is_chargedback`: Tests MoneyTransaction.is_chargedback() through state changes
6. `test_transaction_state_transitions`: Full state machine validation (Normal → Disputed → Chargedback)

#### Integration Tests (5 tests)
Located in `tests/integration_test.rs`:

1. **test_csv_processing_integration**: Full pipeline test with multiple clients
2. **test_single_client_multiple_transactions**: Multiple operations on single client
3. **test_empty_csv**: Edge case handling for empty input
4. **test_dispute_and_resolve**: Tests dispute→resolve flow
5. **test_dispute_and_chargeback**: Tests dispute→chargeback flow

**DataFrame Assertion Logic:**

Integration tests use **Polars DataFrames** to compare actual vs expected CSV output:

```rust
// 1. Run the application and capture stdout
let output = Command::new("cargo").args(&["run", "--", input_csv]).output()

// 2. Parse both actual and expected CSVs into DataFrames
let actual_df = CsvReader::from_path(actual_output).finish()
let expected_df = CsvReader::from_path(expected_csv).finish()

// 3. Sort both by client ID for consistent comparison
let actual_sorted = actual_df.sort(["client"], false, false)
let expected_sorted = expected_df.sort(["client"], false, false)

// 4. Assert shape matches (rows × columns)
assert_eq!(actual_df.shape(), expected_df.shape())

// 5. Assert column names match
assert_eq!(actual_df.get_column_names(), expected_df.get_column_names())

// 6. Assert each column's values match exactly
for col_name in columns {
    assert!(actual_col.equals(expected_col))
}
```

This approach provides:
- **Type-aware comparison**: Respects data types (numbers, booleans, strings)
- **Decimal precision**: Handles floating-point comparisons correctly
- **Clear error messages**: Shows exactly which column/value differs
- **Order independence**: Sorts by client ID before comparison

**Test Structure:**
```
tests/
├── input/              # Input CSV files
├── expected/           # Expected output CSV files
└── integration_test.rs # DataFrame assertion logic
```

### Run Specific Tests
```bash
# Run only unit tests
cargo test --lib

# Run only integration tests
cargo test --test integration_test

# Run specific test
cargo test test_dispute_resolve_cycle
```

## Edge Cases & State Management

### Transaction State Machine

The system enforces strict state transitions for transaction disputes:

```
Transaction Created
    ↓
    ├─→ [Normal State] ──────────────┐
    │                                 │
    └─→ [Dispute] → Disputed State    │
            ↓                         │
            ├─→ [Resolve] ───────────→┘
            │       ↓
            │   Transaction Resolved
            │
            └─→ [Chargeback]
                    ↓
                Account Locked
```

### State Validation Rules

#### 1. **Cannot Resolve/Chargeback Non-Disputed Transactions**
```rust
// ❌ INVALID
deposit(tx: 1, amount: 100)
resolve(tx: 1)  // ERROR: Transaction is not under dispute
```

#### 2. **Cannot Dispute Same Transaction Twice**
```rust
// ❌ INVALID
deposit(tx: 1, amount: 100)
dispute(tx: 1)  // OK
dispute(tx: 1)  // ERROR: Transaction is already under dispute
```

#### 3. **Valid Dispute → Resolve Flow**
```rust
// ✓ VALID
deposit(tx: 1, amount: 100)   // available: 100, held: 0
dispute(tx: 1)                // available: 0, held: 100
resolve(tx: 1)                // available: 100, held: 0
```

#### 4. **Valid Dispute → Chargeback Flow**
```rust
// ✓ VALID
deposit(tx: 1, amount: 100)   // available: 100, total: 100
dispute(tx: 1)                // available: 0, held: 100
chargeback(tx: 1)             // available: 0, total: 0, LOCKED
```

### Other Edge Cases

#### Locked Accounts
- Once an account is locked (after chargeback), it cannot process new transactions
- Chargebacks can still be processed on locked accounts

#### Insufficient Funds
```rust
deposit(amount: 50)
withdrawal(amount: 100)  // ERROR: Insufficient funds
```

#### Negative Amounts
```rust
deposit(amount: -10)  // ERROR: Amount must be positive
```

#### Decimal Precision
```rust
deposit(amount: 1.12345)  // ERROR: Max 4 decimal places
deposit(amount: 1.1234)   // ✓ VALID
```

#### Missing Transaction References
```rust
dispute(tx: 999)  // ERROR: Transaction not found
```

#### Duplicate Transaction IDs
Transaction IDs must be unique per client:
```rust
// ❌ INVALID
deposit(tx: 1, amount: 100)   // OK
deposit(tx: 1, amount: 50)    // ERROR: Transaction ID 1 already exists

// ❌ INVALID - Even across different transaction types
deposit(tx: 1, amount: 100)      // OK
withdrawal(tx: 1, amount: 50)    // ERROR: Transaction ID 1 already exists
```

### Concurrency Safety

The system uses async RwLock for thread-safe account access:
- Multiple reads can happen concurrently
- Writes are exclusive
- No race conditions or data corruption

## Performance Features

- **Buffered Logging**: WriteMode::BufferAndFlush for high throughput
- **Async I/O**: Non-blocking CSV processing
- **Efficient State Tracking**: State encapsulated in transactions for O(1) lookups
- **Decimal Arithmetic**: Precise financial calculations (no floating point)
- **Type-Safe Errors**: Custom TransactionError enum for better error handling

## Dependencies

- `tokio`: Async runtime
- `rust_decimal`: Precise decimal arithmetic
- `csv`: CSV parsing
- `serde`: Serialization
- `flexi_logger`: Flexible logging
- `polars`: DataFrame operations (tests only)

## System Behavior

### CSV Parsing
- **Whitespace normalization**: Leading/trailing spaces trimmed, spaces after commas normalized
- **Case sensitivity**: Transaction types are case-insensitive
- **Empty amounts**: Disputes/resolves/chargebacks don't require amounts (ignored if provided)
- **Invalid rows**: Skipped and logged, processing continues
- **Streaming**: CSV processed line-by-line (doesn't load entire file into memory)

### Output Generation
- **Sorting**: Always sorted by client ID (ascending order)
- **Decimal formatting**: Shows at least 1 decimal place, up to 4 decimal places
- **Headers**: Always includes CSV header row
- **Output channels**: Results to stdout, errors/logs to stderr

### Graceful Shutdown
- **Ctrl-C handling**: Cancels CSV reading, completes in-flight transactions
- **CancellationToken**: Propagates cancellation through async tasks
- **Log flushing**: Ensures all logs written before exit
- **No data loss**: Buffered transactions complete before shutdown

### Transaction Processing Guarantees
- **Order preservation**: Transactions processed in CSV order per client
- **Atomicity**: Each transaction is atomic (all-or-nothing)
- **Isolation**: Client accounts are isolated (no cross-client effects)
- **Idempotency**: Duplicate transaction IDs rejected to prevent double-processing

### Memory Management
- **Channel backpressure**: Limits memory usage to 100 queued transactions
- **Bounded buffer**: Prevents memory exhaustion on large CSV files
- **Clone on output**: Account state cloned for output (no locks held during I/O)

### Logging Configuration
- **Log file**: `./session.log` in current directory
- **Log level**: `info` (warnings and errors included)
- **Write mode**: `BufferAndFlush` for high performance
- **Timestamp**: Suppressed in filename for simplicity

## Error Handling

The application continues processing on errors and logs issues:
- Invalid transactions are skipped
- Error details logged to stderr and `session.log`
- Final account states reflect all successfully processed transactions

**Error Types:**
- **Invalid rows**: Missing client/tx/amount, wrong format
- **Business logic**: Insufficient funds, invalid state transitions
- **Duplicate IDs**: Transaction ID already exists for client

## License

MIT
