use anchor_lang::prelude::*;

/// Singleton PDA — seeds: [b"protocol_config"]
/// Holds global configuration for the Aargau Manager Program.
///
/// Layout: 8 (discriminator) + 104 (data) = 112 bytes total
#[account]
#[derive(Debug)]
pub struct ProtocolConfig {
    /// Admin authority — controls protocol configuration
    pub admin_authority: Pubkey, // 32 bytes
    /// Primary keeper authority
    pub primary_keeper_authority: Pubkey, // 32 bytes
    /// Secondary keeper authority — both required for execute_action (dual-authority)
    pub secondary_keeper_authority: Pubkey, // 32 bytes
    /// Global emergency kill switch — does NOT block emergency_withdraw
    pub is_paused: bool, // 1 byte
    /// Aargau performance fee on LP fees collected (600 = 6%, max 2000 = 20%)
    pub fee_rate_bps: u16, // 2 bytes
    /// On-chain program version — encoded as major * 10_000 + minor * 100 + patch
    pub program_version: u32, // 4 bytes
    /// PDA bump seed
    pub bump: u8, // 1 byte
                  // Total data: 32+32+32+1+2+4+1 = 104 bytes ✓
}

impl ProtocolConfig {
    /// Total account size including discriminator
    pub const LEN: usize = 8 + 104;
    pub const SEEDS: &'static [u8] = b"protocol_config";
    /// Maximum allowed fee rate (20%)
    pub const MAX_FEE_RATE_BPS: u16 = 2000;
}
