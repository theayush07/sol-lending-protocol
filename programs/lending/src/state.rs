use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct LendingMarket {
    pub admin: Pubkey,
    pub paused: bool,
    pub close_factor_bps: u16, // e.g., 5000 = max 50% debt liquidated at once
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Reserve {
    pub market: Pubkey,
    pub liquidity_mint: Pubkey,
    pub liquidity_vault: Pubkey,
    pub oracle: Pubkey,
    pub mint_decimals: u8,
    pub ltv_bps: u16,                   // e.g., 7500 = 75% max initial borrow
    pub liquidation_threshold_bps: u16, // e.g., 8000 = 80% liquidation trigger
    pub liquidation_bonus_bps: u16,     // e.g., 500  = 5% discount for liquidator
    pub base_borrow_rate_bps: u16,
    pub slope_borrow_rate_bps: u16,
    pub total_deposits: u64,
    pub total_borrowed_wad: u128,
    pub cumulative_borrow_rate_wad: u128,
    pub last_update_slot: u64,
    pub bump: u8,
    pub vault_bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Obligation {
    pub market: Pubkey,
    pub owner: Pubkey,
    pub collateral_reserve: Pubkey,
    pub borrow_reserve: Pubkey,
    pub deposited_collateral: u64,
    pub borrowed_amount_wad: u128,
    pub entry_borrow_rate_wad: u128,
    pub last_update_slot: u64,
    pub bump: u8,
}