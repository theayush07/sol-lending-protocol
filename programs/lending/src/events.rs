use anchor_lang::prelude::*;

#[event]
pub struct CollateralDeposited {
    pub obligation: Pubkey,
    pub owner: Pubkey,
    pub reserve: Pubkey,
    pub amount: u64,
    pub total_collateral: u64,
    pub slot: u64,
}

#[event]
pub struct LiquidityBorrowed {
    pub obligation: Pubkey,
    pub owner: Pubkey,
    pub reserve: Pubkey,
    pub amount: u64,
    pub total_borrowed_wad: u128,
    pub health_factor_bps: u64,
    pub slot: u64,
}

#[event]
pub struct LiquidityRepaid {
    pub obligation: Pubkey,
    pub payer: Pubkey,
    pub reserve: Pubkey,
    pub amount: u64,
    pub remaining_borrowed_wad: u128,
    pub slot: u64,
}

#[event]
pub struct PositionLiquidated {
    pub obligation: Pubkey,
    pub liquidator: Pubkey,
    pub debt_reserve: Pubkey,
    pub collateral_reserve: Pubkey,
    pub repaid_amount: u64,
    pub seized_collateral: u64,
    pub health_factor_before_bps: u64,
    pub slot: u64,
}