use anchor_lang::prelude::*;

#[error_code]
pub enum LendingError {
    #[msg("Amount must be greater than zero")]
    InvalidAmount,
    #[msg("Math overflow or underflow occurred")]
    MathOverflow,
    #[msg("Market is currently paused for emergency maintenance")]
    MarketPaused,
    #[msg("Oracle price feed is stale or uninitialized")]
    StaleOraclePrice,
    #[msg("Oracle confidence interval is too wide")]
    OracleConfidenceTooWide,
    #[msg("Insufficient collateral to cover requested borrow (LTV exceeded)")]
    InsufficientCollateral,
    #[msg("Reserve does not have enough available liquidity")]
    InsufficientLiquidity,
    #[msg("Obligation is healthy and cannot be liquidated")]
    ObligationHealthy,
    #[msg("Liquidation amount exceeds maximum close factor (50%)")]
    LiquidationTooLarge,
    #[msg("Invalid reserve configuration parameters")]
    InvalidReserveConfig,
    #[msg("Oracle account does not match the reserve configuration")]
    OracleMismatch,
}