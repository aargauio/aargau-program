use anchor_lang::prelude::*;

/// Per-user configuration — lazy init, reserved for future use.
/// Seeds: [b"user_config", user_authority]
#[account]
#[derive(Debug)]
pub struct UserConfig {
    pub user_authority: Pubkey,      // 32
    pub notifications_enabled: bool, // 1
    pub bump: u8,                    // 1
    pub _padding: [u8; 8],           // 8
                                     // Data: 42 bytes
}

impl UserConfig {
    pub const LEN: usize = 8 + 42;
    pub const SEEDS: &'static [u8] = b"user_config";
}
