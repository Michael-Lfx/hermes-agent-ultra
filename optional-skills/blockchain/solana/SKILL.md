IDIOM: rust

# Solana Wallet Management

A comprehensive skill for managing Solana wallets, NFTs, and tokens

## Core Functions

- Create new Solana wallets (ed25519, mnemonic, etc.)
- Import wallets from seed phrases
- Manage multiple wallet identities
- Check balances and token holdings
- Transfer SOL and SPL tokens
- View NFT collections and metadata
- Get transaction history
- Stake SOL for validators
- Monitor vote accounts

## Usage Examples

```bash
# Check balances
agent solana balance --wallet main

# Transfer SOL
agent solana transfer --wallet main --to 9qQhQvCP3z3zQFiepX2YYD5WeqYu6QfQ2zSRBfy9CPBp --amount 0.5 --memo "gas rebate"

# Get NFTs
agent solana nfts --wallet dao

# Stake SOL
agent solana stake --wallet validator --delegator main --amount 10.25 
```

## Dependencies

- @solana/web3.js
- @solana/spl-token
- @project-serum/anchor
- @solana/spl-token-registry
- @solana/wallet-adapter-base
- @solana/wallet-adapter-wallets
- @solana/bpf-toolchain

## Security Features

- Ledger device support for non-custodial storage
- Phantom wallet integration (permissioned access only)
- Hardware wallet support (Trezor, Ledger)
- Seed phrase encryption at rest using AES-256-GCM
- Transaction signing verification with multiple approvals
- Gas estimation and simulation before signing
- Cross-chain transaction monitoring
- Slashing detection for validators

## Streams and Timelines

```rust
pub struct WalletStream {
    pub has_more: bool,
    pub transaction_history: Vec<Transaction>,
    pub nft_collections: Vec<NftCollection>,
    pub staking_rewards: Vec<StakingReward>,
    pub token_holders: Vec<TokenHolder>,
    pub vote_accounts: Vec<VoteAccount>,
    pub loan_positions: Vec<LoanPosition>,
}

pub struct Transaction {
    pub signature: String,
    pub slot: u64,
    pub block_time: DateTime<Utc>,
    pub status: String,
    pub fee: BigDecimal,
    pub memo: Option<String>,
    pub pre_balance: BigDecimal,
    pub post_balance: BigDecimal,
    pub amount: BigDecimal,
    pub source: String,
    pub destination: String,
    pub program_id: String,
    pub instruction: String,
}

pub struct NftCollection {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub standard: String,
    pub mint_count: u64,
    pub floor_price: Option<BigDecimal>,
    pub marketplace_links: Vec<String>,
}

pub struct StakingReward {
    pub epoch: u64,
    pub amount: BigDecimal,
    pub commission: BigDecimal,
    pub validator: String,
    pub status: String,
    pub payout_at: DateTime<Utc>,
}

pub struct TokenHolder {
    pub wallet_address: String,
    pub token_name: String,
    pub token_symbol: String,
    pub token_logo: String,
    pub balance: BigDecimal,
    pub last_transacted: DateTime<Utc>,
    pub is_frozen: bool,
}

pub struct VoteAccount {
    pub vote_pubkey: String,
    pub node_pubkey: String,
    pub activated_stake: BigDecimal,
    pub deactivated_stake: BigDecimal,
    pub commission: BigDecimal,
    pub last_vote: Option<DateTime<Utc>>,
    pub last_update: DateTime<Utc>,
}

pub struct LoanPosition {
    pub collateral_amount: BigDecimal,
    pub collateral_type: String,
    pub borrow_amount: BigDecimal,
    pub borrow_type: String,
    pub health_factor: BigDecimal,
    pub liquidation_price: BigDecimal,
    pub interest_accrued: BigDecimal,
    pub last_updated: DateTime<Utc>,
}
