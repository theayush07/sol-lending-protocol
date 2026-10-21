use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use anchor_lang::prelude::borsh::BorshDeserialize;
use mock_oracle::{PriceFeedAccount, ORACLE_MAGIC};

pub mod errors;
pub mod events;
pub mod math;
pub mod state;

use errors::LendingError;
use events::*;
use math::*;
use state::*;

declare_id!("3cqF12e75uNGrM9MnmDHaeYcRpcy8GVjRyBxC4MVakfZ");

/// Maximum allowed oracle age in slots (~60 seconds at 400ms/slot)
pub const MAX_ORACLE_STALENESS_SLOTS: u64 = 150;

#[program]
pub mod lending {
    use super::*;

    pub fn init_market(ctx: Context<InitMarket>, close_factor_bps: u16) -> Result<()> {
        require!(
            close_factor_bps > 0 && close_factor_bps <= 10_000,
            LendingError::InvalidReserveConfig
        );
        let market = &mut ctx.accounts.market;
        market.admin = ctx.accounts.admin.key();
        market.paused = false;
        market.close_factor_bps = close_factor_bps;
        market.bump = ctx.bumps.market;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn init_reserve(
        ctx: Context<InitReserve>,
        ltv_bps: u16,
        liquidation_threshold_bps: u16,
        liquidation_bonus_bps: u16,
        base_borrow_rate_bps: u16,
        slope_borrow_rate_bps: u16,
    ) -> Result<()> {
        require!(
            ltv_bps < liquidation_threshold_bps && liquidation_threshold_bps <= 9500,
            LendingError::InvalidReserveConfig
        );
        let clock = Clock::get()?;
        let reserve = &mut ctx.accounts.reserve;
        reserve.market = ctx.accounts.market.key();
        reserve.liquidity_mint = ctx.accounts.liquidity_mint.key();
        reserve.liquidity_vault = ctx.accounts.liquidity_vault.key();
        reserve.oracle = ctx.accounts.oracle.key();
        reserve.mint_decimals = ctx.accounts.liquidity_mint.decimals;
        reserve.ltv_bps = ltv_bps;
        reserve.liquidation_threshold_bps = liquidation_threshold_bps;
        reserve.liquidation_bonus_bps = liquidation_bonus_bps;
        reserve.base_borrow_rate_bps = base_borrow_rate_bps;
        reserve.slope_borrow_rate_bps = slope_borrow_rate_bps;
        reserve.total_deposits = 0;
        reserve.total_borrowed_wad = 0;
        reserve.cumulative_borrow_rate_wad = WAD;
        reserve.last_update_slot = clock.slot;
        reserve.bump = ctx.bumps.reserve;
        reserve.vault_bump = ctx.bumps.liquidity_vault;
        Ok(())
    }

    pub fn init_obligation(ctx: Context<InitObligation>) -> Result<()> {
        let clock = Clock::get()?;
        let ob = &mut ctx.accounts.obligation;
        ob.market = ctx.accounts.market.key();
        ob.owner = ctx.accounts.owner.key();
        ob.collateral_reserve = Pubkey::default();
        ob.borrow_reserve = Pubkey::default();
        ob.deposited_collateral = 0;
        ob.borrowed_amount_wad = 0;
        ob.entry_borrow_rate_wad = WAD;
        ob.last_update_slot = clock.slot;
        ob.bump = ctx.bumps.obligation;
        Ok(())
    }

    pub fn deposit_collateral(ctx: Context<DepositCollateral>, amount: u64) -> Result<()> {
        require!(!ctx.accounts.market.paused, LendingError::MarketPaused);
        require!(amount > 0, LendingError::InvalidAmount);

        let clock = Clock::get()?;
        let reserve = &mut ctx.accounts.reserve;
        accrue_reserve_interest(reserve, clock.slot)?;

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user_token_account.to_account_info(),
                    to: ctx.accounts.liquidity_vault.to_account_info(),
                    authority: ctx.accounts.owner.to_account_info(),
                },
            ),
            amount,
        )?;

        reserve.total_deposits = reserve
            .total_deposits
            .checked_add(amount)
            .ok_or(LendingError::MathOverflow)?;

        let ob = &mut ctx.accounts.obligation;
        if ob.collateral_reserve == Pubkey::default() {
            ob.collateral_reserve = reserve.key();
        } else {
            require_keys_eq!(ob.collateral_reserve, reserve.key());
        }

        ob.deposited_collateral = ob
            .deposited_collateral
            .checked_add(amount)
            .ok_or(LendingError::MathOverflow)?;
        ob.last_update_slot = clock.slot;

        emit!(CollateralDeposited {
            obligation: ob.key(),
            owner: ob.owner,
            reserve: reserve.key(),
            amount,
            total_collateral: ob.deposited_collateral,
            slot: clock.slot,
        });

        Ok(())
    }

    pub fn borrow(ctx: Context<Borrow>, amount: u64) -> Result<()> {
        require!(!ctx.accounts.market.paused, LendingError::MarketPaused);
        require!(amount > 0, LendingError::InvalidAmount);
        require!(
            ctx.accounts.borrow_vault.amount >= amount,
            LendingError::InsufficientLiquidity
        );

        let clock = Clock::get()?;
        let col_reserve = &mut ctx.accounts.collateral_reserve;
        let bor_reserve = &mut ctx.accounts.borrow_reserve;
        let ob = &mut ctx.accounts.obligation;

        accrue_reserve_interest(bor_reserve, clock.slot)?;
        sync_obligation_debt(ob, bor_reserve.cumulative_borrow_rate_wad)?;

        if ob.borrow_reserve == Pubkey::default() {
            ob.borrow_reserve = bor_reserve.key();
        } else {
            require_keys_eq!(ob.borrow_reserve, bor_reserve.key());
        }

        let col_price = read_validated_oracle(&ctx.accounts.collateral_oracle, col_reserve.oracle, clock.slot)?;
        let bor_price = read_validated_oracle(&ctx.accounts.borrow_oracle, bor_reserve.oracle, clock.slot)?;

        let added_debt_wad = (amount as u128)
            .checked_mul(WAD)
            .ok_or(LendingError::MathOverflow)?;
        let new_debt_wad = ob
            .borrowed_amount_wad
            .checked_add(added_debt_wad)
            .ok_or(LendingError::MathOverflow)?;
        let new_debt_tokens = new_debt_wad.div_ceil(WAD);

        let col_usd = token_usd_value_micro(
            ob.deposited_collateral as u128,
            col_reserve.mint_decimals,
            col_price,
            false,
        )?;
        let debt_usd = token_usd_value_micro(
            new_debt_tokens,
            bor_reserve.mint_decimals,
            bor_price,
            true,
        )?;

        let max_borrow_usd = col_usd
            .checked_mul(col_reserve.ltv_bps as u128)
            .ok_or(LendingError::MathOverflow)?
            .checked_div(BPS_SCALE)
            .ok_or(LendingError::MathOverflow)?;

        require!(debt_usd <= max_borrow_usd, LendingError::InsufficientCollateral);

        ob.borrowed_amount_wad = new_debt_wad;
        ob.last_update_slot = clock.slot;
        bor_reserve.total_borrowed_wad = bor_reserve
            .total_borrowed_wad
            .checked_add(added_debt_wad)
            .ok_or(LendingError::MathOverflow)?;

        let bor_reserve_key = bor_reserve.key();
        let seeds = &[
            b"vault".as_ref(),
            bor_reserve_key.as_ref(),
            &[bor_reserve.vault_bump],
        ];
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.borrow_vault.to_account_info(),
                    to: ctx.accounts.user_receive_token_account.to_account_info(),
                    authority: ctx.accounts.borrow_vault.to_account_info(),
                },
                &[seeds],
            ),
            amount,
        )?;

        let hf_bps = health_factor_bps(col_usd, col_reserve.liquidation_threshold_bps, debt_usd)?;

        emit!(LiquidityBorrowed {
            obligation: ob.key(),
            owner: ob.owner,
            reserve: bor_reserve.key(),
            amount,
            total_borrowed_wad: ob.borrowed_amount_wad,
            health_factor_bps: hf_bps,
            slot: clock.slot,
        });

        Ok(())
    }

    pub fn repay(ctx: Context<Repay>, amount: u64) -> Result<()> {
        require!(amount > 0, LendingError::InvalidAmount);
        let clock = Clock::get()?;
        let bor_reserve = &mut ctx.accounts.borrow_reserve;
        let ob = &mut ctx.accounts.obligation;

        accrue_reserve_interest(bor_reserve, clock.slot)?;
        sync_obligation_debt(ob, bor_reserve.cumulative_borrow_rate_wad)?;

        let current_debt_tokens = (ob.borrowed_amount_wad.div_ceil(WAD)) as u64;
        let actual_repay = amount.min(current_debt_tokens);
        let repay_wad = (actual_repay as u128)
            .checked_mul(WAD)
            .ok_or(LendingError::MathOverflow)?;

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.payer_token_account.to_account_info(),
                    to: ctx.accounts.borrow_vault.to_account_info(),
                    authority: ctx.accounts.payer.to_account_info(),
                },
            ),
            actual_repay,
        )?;

        ob.borrowed_amount_wad = ob.borrowed_amount_wad.saturating_sub(repay_wad);
        bor_reserve.total_borrowed_wad = bor_reserve.total_borrowed_wad.saturating_sub(repay_wad);
        ob.last_update_slot = clock.slot;

        emit!(LiquidityRepaid {
            obligation: ob.key(),
            payer: ctx.accounts.payer.key(),
            reserve: bor_reserve.key(),
            amount: actual_repay,
            remaining_borrowed_wad: ob.borrowed_amount_wad,
            slot: clock.slot,
        });

        Ok(())
    }

    pub fn liquidate(ctx: Context<Liquidate>, repay_amount: u64) -> Result<()> {
        require!(!ctx.accounts.market.paused, LendingError::MarketPaused);
        require!(repay_amount > 0, LendingError::InvalidAmount);

        let clock = Clock::get()?;
        let col_reserve = &mut ctx.accounts.collateral_reserve;
        let bor_reserve = &mut ctx.accounts.borrow_reserve;
        let ob = &mut ctx.accounts.obligation;

        accrue_reserve_interest(bor_reserve, clock.slot)?;
        sync_obligation_debt(ob, bor_reserve.cumulative_borrow_rate_wad)?;

        let col_price = read_validated_oracle(&ctx.accounts.collateral_oracle, col_reserve.oracle, clock.slot)?;
        let bor_price = read_validated_oracle(&ctx.accounts.borrow_oracle, bor_reserve.oracle, clock.slot)?;

        let debt_tokens = ob.borrowed_amount_wad.div_ceil(WAD);
        let col_usd = token_usd_value_micro(
            ob.deposited_collateral as u128,
            col_reserve.mint_decimals,
            col_price,
            false,
        )?;
        let debt_usd = token_usd_value_micro(
            debt_tokens,
            bor_reserve.mint_decimals,
            bor_price,
            true,
        )?;

        let hf_before = health_factor_bps(col_usd, col_reserve.liquidation_threshold_bps, debt_usd)?;
        require!(hf_before < 10_000, LendingError::ObligationHealthy);

        let max_repay = (debt_tokens
            .checked_mul(ctx.accounts.market.close_factor_bps as u128)
            .ok_or(LendingError::MathOverflow)?
            .div_ceil(BPS_SCALE)) as u64;
        require!(repay_amount <= max_repay, LendingError::LiquidationTooLarge);

        // Calculate collateral to seize with liquidation bonus
        let repay_usd = token_usd_value_micro(
            repay_amount as u128,
            bor_reserve.mint_decimals,
            bor_price,
            false,
        )?;
        let bonus_multiplier_bps = BPS_SCALE
            .checked_add(col_reserve.liquidation_bonus_bps as u128)
            .ok_or(LendingError::MathOverflow)?;
        let seize_usd = repay_usd
            .checked_mul(bonus_multiplier_bps)
            .ok_or(LendingError::MathOverflow)?
            .checked_div(BPS_SCALE)
            .ok_or(LendingError::MathOverflow)?;

        let col_scale = 10u128.pow(col_reserve.mint_decimals as u32);
        let seized_collateral = ((seize_usd
            .checked_mul(col_scale)
            .ok_or(LendingError::MathOverflow)?)
            / (col_price as u128)) as u64;
        let seized_capped = seized_collateral.min(ob.deposited_collateral);

        // 1. Transfer repaid debt from liquidator to borrow vault
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.liquidator_debt_account.to_account_info(),
                    to: ctx.accounts.borrow_vault.to_account_info(),
                    authority: ctx.accounts.liquidator.to_account_info(),
                },
            ),
            repay_amount,
        )?;

        // 2. Transfer seized collateral from collateral vault to liquidator
        let col_reserve_key = col_reserve.key();
        let col_seeds = &[
            b"vault".as_ref(),
            col_reserve_key.as_ref(),
            &[col_reserve.vault_bump],
        ];
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.collateral_vault.to_account_info(),
                    to: ctx.accounts.liquidator_collateral_account.to_account_info(),
                    authority: ctx.accounts.collateral_vault.to_account_info(),
                },
                &[col_seeds],
            ),
            seized_capped,
        )?;

        let repay_wad = (repay_amount as u128) * WAD;
        ob.borrowed_amount_wad = ob.borrowed_amount_wad.saturating_sub(repay_wad);
        bor_reserve.total_borrowed_wad = bor_reserve.total_borrowed_wad.saturating_sub(repay_wad);
        ob.deposited_collateral = ob.deposited_collateral.saturating_sub(seized_capped);
        col_reserve.total_deposits = col_reserve.total_deposits.saturating_sub(seized_capped);
        ob.last_update_slot = clock.slot;

        emit!(PositionLiquidated {
            obligation: ob.key(),
            liquidator: ctx.accounts.liquidator.key(),
            debt_reserve: bor_reserve.key(),
            collateral_reserve: col_reserve.key(),
            repaid_amount: repay_amount,
            seized_collateral: seized_capped,
            health_factor_before_bps: hf_before,
            slot: clock.slot,
        });

        Ok(())
    }
}

fn accrue_reserve_interest(reserve: &mut Account<Reserve>, current_slot: u64) -> Result<()> {
    let slots_elapsed = current_slot.saturating_sub(reserve.last_update_slot);
    if slots_elapsed == 0 {
        return Ok(());
    }
    let util_bps = utilization_rate_bps(reserve.total_deposits, reserve.total_borrowed_wad)?;
    let new_index = compound_borrow_index(
        reserve.cumulative_borrow_rate_wad,
        reserve.base_borrow_rate_bps,
        reserve.slope_borrow_rate_bps,
        util_bps,
        slots_elapsed,
    )?;

    if reserve.total_borrowed_wad > 0 {
        // Safe u128 multiplication via quotient/remainder decomposition
        let q = reserve.total_borrowed_wad / reserve.cumulative_borrow_rate_wad;
        let r = reserve.total_borrowed_wad % reserve.cumulative_borrow_rate_wad;
        
        let base = q.checked_mul(new_index).ok_or(LendingError::MathOverflow)?;
        let remainder = r
            .checked_mul(new_index)
            .ok_or(LendingError::MathOverflow)?
            .checked_div(reserve.cumulative_borrow_rate_wad)
            .ok_or(LendingError::MathOverflow)?;
            
        reserve.total_borrowed_wad = base.checked_add(remainder).ok_or(LendingError::MathOverflow)?;
    }

    reserve.cumulative_borrow_rate_wad = new_index;
    reserve.last_update_slot = current_slot;
    Ok(())
}

fn sync_obligation_debt(ob: &mut Account<Obligation>, reserve_index_wad: u128) -> Result<()> {
    if ob.borrowed_amount_wad > 0 && ob.entry_borrow_rate_wad > 0 {
        // Safe u128 multiplication via quotient/remainder decomposition
        let q = ob.borrowed_amount_wad / ob.entry_borrow_rate_wad;
        let r = ob.borrowed_amount_wad % ob.entry_borrow_rate_wad;
        
        let base = q.checked_mul(reserve_index_wad).ok_or(LendingError::MathOverflow)?;
        let remainder = r
            .checked_mul(reserve_index_wad)
            .ok_or(LendingError::MathOverflow)?
            .checked_div(ob.entry_borrow_rate_wad)
            .ok_or(LendingError::MathOverflow)?;
            
        ob.borrowed_amount_wad = base.checked_add(remainder).ok_or(LendingError::MathOverflow)?;
    }
    ob.entry_borrow_rate_wad = reserve_index_wad;
    Ok(())
}

fn read_validated_oracle(
    oracle_info: &AccountInfo,
    expected_key: Pubkey,
    current_slot: u64,
) -> Result<u64> {
    require_keys_eq!(oracle_info.key(), expected_key, LendingError::OracleMismatch);
    let data = oracle_info.try_borrow_data()?;
    require!(
        data.len() >= PriceFeedAccount::LEN,
        LendingError::StaleOraclePrice
    );
    let feed = PriceFeedAccount::try_from_slice(&data[..PriceFeedAccount::LEN])
        .map_err(|_| error!(LendingError::StaleOraclePrice))?;

    require!(feed.magic == ORACLE_MAGIC, LendingError::StaleOraclePrice);
    require!(feed.price_usd_micro > 0, LendingError::StaleOraclePrice);
    require!(
        current_slot.saturating_sub(feed.last_updated_slot) <= MAX_ORACLE_STALENESS_SLOTS,
        LendingError::StaleOraclePrice
    );
    // Reject if confidence interval is wider than 5% of price
    require!(
        feed.confidence_micro * 20 <= feed.price_usd_micro,
        LendingError::OracleConfidenceTooWide
    );
    Ok(feed.price_usd_micro)
}

#[derive(Accounts)]
pub struct InitMarket<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + LendingMarket::INIT_SPACE,
        seeds = [b"market"],
        bump
    )]
    pub market: Account<'info, LendingMarket>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitReserve<'info> {
    #[account(has_one = admin)]
    pub market: Box<Account<'info, LendingMarket>>,
    #[account(
        init,
        payer = admin,
        space = 8 + Reserve::INIT_SPACE,
        seeds = [b"reserve", market.key().as_ref(), liquidity_mint.key().as_ref()],
        bump
    )]
    pub reserve: Box<Account<'info, Reserve>>,
    pub liquidity_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = admin,
        token::mint = liquidity_mint,
        token::authority = liquidity_vault,
        seeds = [b"vault", reserve.key().as_ref()],
        bump
    )]
    pub liquidity_vault: Box<Account<'info, TokenAccount>>,
    /// CHECK: Validated on price reads against `mock_oracle` layout
    pub oracle: UncheckedAccount<'info>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitObligation<'info> {
    pub market: Account<'info, LendingMarket>,
    #[account(
        init,
        payer = owner,
        space = 8 + Obligation::INIT_SPACE,
        seeds = [b"obligation", market.key().as_ref(), owner.key().as_ref()],
        bump
    )]
    pub obligation: Account<'info, Obligation>,
    #[account(mut)]
    pub owner: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DepositCollateral<'info> {
    pub market: Box<Account<'info, LendingMarket>>,
    #[account(mut, has_one = market, has_one = liquidity_vault)]
    pub reserve: Box<Account<'info, Reserve>>,
    #[account(mut, has_one = market, has_one = owner)]
    pub obligation: Box<Account<'info, Obligation>>,
    #[account(mut)]
    pub liquidity_vault: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub user_token_account: Box<Account<'info, TokenAccount>>,
    pub owner: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Borrow<'info> {
    pub market: Box<Account<'info, LendingMarket>>,
    #[account(mut, has_one = market)]
    pub collateral_reserve: Box<Account<'info, Reserve>>,
    #[account(mut, has_one = market)]
    pub borrow_reserve: Box<Account<'info, Reserve>>,
    #[account(mut, has_one = market, has_one = owner)]
    pub obligation: Box<Account<'info, Obligation>>,
    #[account(
        mut,
        address = borrow_reserve.liquidity_vault
    )]
    pub borrow_vault: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
   pub user_receive_token_account: Box<Account<'info, TokenAccount>>,
    /// CHECK: Verified in `read_validated_oracle`
    pub collateral_oracle: UncheckedAccount<'info>,
    /// CHECK: Verified in `read_validated_oracle`
    pub borrow_oracle: UncheckedAccount<'info>,
    pub owner: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Repay<'info> {
    #[account(mut)]
    pub borrow_reserve: Box<Account<'info, Reserve>>,
    #[account(mut, has_one = borrow_reserve)]
    pub obligation: Box<Account<'info, Obligation>>,
    #[account(mut, address = borrow_reserve.liquidity_vault)]
    pub borrow_vault: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub payer_token_account: Box<Account<'info, TokenAccount>>,
    pub payer: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Liquidate<'info> {
    pub market: Box<Account<'info, LendingMarket>>,
    #[account(mut, has_one = market)]
    pub collateral_reserve: Box<Account<'info, Reserve>>,
    #[account(mut, has_one = market)]
    pub borrow_reserve: Box<Account<'info, Reserve>>,
    #[account(
        mut,
        has_one = market,
        has_one = collateral_reserve,
        has_one = borrow_reserve
    )]
    pub obligation: Box<Account<'info, Obligation>>,
    #[account(mut, address = collateral_reserve.liquidity_vault)]
    pub collateral_vault: Box<Account<'info, TokenAccount>>,
    #[account(mut, address = borrow_reserve.liquidity_vault)]
    pub borrow_vault: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub liquidator_debt_account: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub liquidator_collateral_account: Box<Account<'info, TokenAccount>>,
    /// CHECK: Verified in `read_validated_oracle`
    pub collateral_oracle: UncheckedAccount<'info>,
    /// CHECK: Verified in `read_validated_oracle`
    pub borrow_oracle: UncheckedAccount<'info>,
    pub liquidator: Signer<'info>,
    pub token_program: Program<'info, Token>,
}