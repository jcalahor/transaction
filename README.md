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
Located in `src/account.rs`, `src/csv.rs`, and `src/transaction.rs`:

**Account Tests (28 tests):**
- Account creation and basic operations
- Deposit/withdrawal logic
- Insufficient funds handling
- Dispute/resolve/chargeback flows
- **State transition validation** (5 tests)
- **Transaction ID uniqueness validation** (3 tests)
- **Multiple chargebacks on locked accounts** (1 test)
- **Locked account rejection** (1 test)
- **Ledger state query methods** (3 tests: is_disputed, is_chargedback, nonexistent tx)
- **Dispute-resolve-dispute cycle** (1 test): Verifies transactions can be re-disputed after resolution

**CSV Tests (4 tests):**
- Transaction type parsing
- Amount validation
- Error handling for invalid data
- Whitespace trimming

**Transaction Tests (6 tests):**
- Money transaction creation and validation
- Transaction state queries (is_disputed, is_chargedback)
- Full state transition testing (Normal → Disputed → Chargedback)
- Transaction client ID extraction

#### Integration Tests (5 tests)
Located in `tests/integration_test.rs`:

1. **test_csv_processing_integration**: Full pipeline test with multiple clients
2. **test_single_client_multiple_transactions**: Multiple operations on single client
3. **test_empty_csv**: Edge case handling for empty input
4. **test_dispute_and_resolve**: Tests dispute→resolve flow
   - Deposits funds, disputes a transaction, then resolves it
   - Verifies funds move from available→held→available correctly
5. **test_dispute_and_chargeback**: Tests dispute→chargeback flow
   - Deposits funds, disputes a transaction, then processes chargeback
   - Verifies account is locked and funds are permanently removed

**Test Structure:**
```
tests/
├── input/              # Input CSV files
│   ├── test_data.csv
│   ├── single_client.csv
│   ├── empty.csv
│   ├── dispute_resolve.csv
│   └── dispute_chargeback.csv
├── expected/           # Expected output CSV files
│   ├── test_data_expected.csv
│   ├── single_client_expected.csv
│   ├── empty_expected.csv
│   ├── dispute_resolve_expected.csv
│   └── dispute_chargeback_expected.csv
└── integration_test.rs
```

Integration tests use **Polars DataFrames** for precise CSV comparison.

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

## Error Handling

The application continues processing on errors and logs issues:
- Invalid transactions are skipped
- Error details logged to stderr and `session.log`
- Final account states reflect all successfully processed transactions

## License

MIT
