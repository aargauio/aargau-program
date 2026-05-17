use anchor_lang::prelude::*;

#[error_code]
pub enum AargauError {
    // --- Input validation (1000–1099) ---
    #[msg("Deposit amount must be non-zero for at least one token")]
    DepositAmountZero = 1000,

    #[msg("Withdrawal percentage bps must be between 1 and 10000")]
    InvalidWithdrawPctBps = 1001,

    #[msg("Received amount is below slippage minimum")]
    SlippageExceeded = 1002,

    #[msg("Meteora strategy exceeds maximum of 69 bins")]
    TooManyBins = 1003,

    #[msg("Fee rate exceeds maximum of 2000 bps (20%)")]
    FeeRateTooHigh = 1004,

    #[msg("Public key must not be the default (zero) pubkey")]
    InvalidPublicKey = 1005,

    #[msg("Vault token balance is insufficient for the requested action")]
    InsufficientFunds = 1006,

    #[msg("KeeperAction payload is missing or out of allowed bounds")]
    InvalidActionPayload = 1007,

    #[msg(
        "Token-2022 mints are not supported — transfer hooks not yet wired into the fee transfer path"
    )]
    Token2022NotSupported = 1008,

    #[msg("Pool reserve account does not match the lb_pair on-chain reserve pubkey")]
    InvalidPoolReserve = 1009,

    #[msg("active_id_slippage exceeds the hard cap")]
    SlippageOutOfRange = 1010,

    // --- Authority (1100–1199) ---
    #[msg("Signer is not the vault user authority")]
    UnauthorizedUser = 1100,

    #[msg("Signer is not the protocol admin authority")]
    UnauthorizedAdmin = 1101,

    #[msg("Primary and secondary keeper authorities must differ")]
    DuplicateKeeperAuthority = 1102,

    #[msg("Signer is not an authorized keeper authority")]
    UnauthorizedKeeper = 1103,

    // --- Protocol / Pool validation (1200–1299) ---
    #[msg("Pool account is not owned by the expected protocol program")]
    InvalidPool = 1200,

    #[msg("Pool account discriminator does not match expected protocol")]
    InvalidPoolDiscriminator = 1201,

    #[msg("Pool mint does not match vault mint")]
    InvalidPoolMint = 1202,

    #[msg("Mint has NonTransferable extension — not supported")]
    NonTransferableMint = 1203,

    #[msg("Pool operation is disabled by Raydium status bitmask")]
    PoolOperationDisabled = 1204,

    #[msg("Meteora position account is not a PositionV2 (wrong discriminator or owning program)")]
    InvalidPositionDiscriminator = 1205,

    #[msg(
        "Bin range exceeds the inline LbPair bitmap; bin_array_bitmap_extension support is not implemented"
    )]
    BitmapExtensionRequired = 1206,

    // --- Vault state (1300–1399) ---
    #[msg("Vault PDA derivation mismatch")]
    InvalidVaultPda = 1300,

    #[msg("Vault has an active LP position")]
    VaultHasActivePosition = 1301,

    #[msg("Vault has no active LP position")]
    VaultNoActivePosition = 1302,

    #[msg("A pending rebalance already exists — cancel or complete it first")]
    PendingRebalanceExists = 1303,

    #[msg("No pending rebalance found")]
    NoPendingRebalance = 1304,

    #[msg("Pending rebalance has not reached its timeout yet")]
    PendingRebalanceNotExpired = 1305,

    #[msg("Vault token balance decreased after CPI when it should have increased")]
    PostCpiBalanceDecreased = 1306,

    #[msg("Computed performance fee exceeds gross amount")]
    FeeExceedsGross = 1307,

    // --- Protocol state (1400–1499) ---
    #[msg("Program is paused — only emergency_withdraw, withdraw, and close_vault are allowed")]
    ProgramPaused = 1400,

    #[msg("Rewards are not supported for this protocol")]
    RewardsNotSupportedForProtocol = 1401,

    // --- Treasury (1500–1599) ---
    #[msg("Treasury token account owner does not match treasury PDA")]
    InvalidTreasury = 1500,

    #[msg("Treasury invariant violated — actual delta differs from expected net")]
    TreasuryInvariantViolated = 1501,

    // --- Arithmetic (1600–1699) ---
    #[msg("Arithmetic overflow")]
    Overflow = 1600,

    #[msg("Arithmetic underflow")]
    Underflow = 1601,

    #[msg("Division by zero")]
    DivisionByZero = 1602,
}
