use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::Sysvar,
};

solana_program::declare_id!("BSKYQxSA7STmDxYGqasHmb3Yqa5a6DeSGHtfs1oK58r9");

/// Magic header (`0x50595448` = "PYTH") so consumers can verify account initialization
pub const ORACLE_MAGIC: u32 = 0x5059_5448;

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceFeedAccount {
    pub magic: u32,
    pub authority: Pubkey,
    /// Price scaled by 10^6 (e.g. $150.00 = 150_000_000)
    pub price_usd_micro: u64,
    pub confidence_micro: u64,
    pub last_updated_slot: u64,
    pub last_updated_ts: i64,
}

impl PriceFeedAccount {
    pub const LEN: usize = 4 + 32 + 8 + 8 + 8 + 8; // 68 bytes
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum OracleInstruction {
    /// Accounts:
    /// 0. `[writable]` Price feed account (pre-allocated by caller, owned by this program)
    /// 1. `[signer]`   Authority
    Initialize { initial_price_micro: u64, confidence_micro: u64 },

    /// Accounts:
    /// 0. `[writable]` Price feed account
    /// 1. `[signer]`   Authority
    SetPrice { price_micro: u64, confidence_micro: u64 },
}

#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let ix = OracleInstruction::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    let account_iter = &mut accounts.iter();
    let feed_info = next_account_info(account_iter)?;
    let authority_info = next_account_info(account_iter)?;

    if feed_info.owner != program_id {
        msg!("Oracle account not owned by mock_oracle program");
        return Err(ProgramError::IncorrectProgramId);
    }
    if !authority_info.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if !feed_info.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }

    let clock = Clock::get()?;

    match ix {
        OracleInstruction::Initialize {
            initial_price_micro,
            confidence_micro,
        } => {
            if feed_info.data_len() < PriceFeedAccount::LEN {
                return Err(ProgramError::AccountDataTooSmall);
            }
            let mut data = feed_info.try_borrow_mut_data()?;
            let existing_magic = u32::from_le_bytes(data[0..4].try_into().unwrap());
            if existing_magic == ORACLE_MAGIC {
                return Err(ProgramError::AccountAlreadyInitialized);
            }

            let state = PriceFeedAccount {
                magic: ORACLE_MAGIC,
                authority: *authority_info.key,
                price_usd_micro: initial_price_micro,
                confidence_micro,
                last_updated_slot: clock.slot,
                last_updated_ts: clock.unix_timestamp,
            };
            state.serialize(&mut &mut data[..PriceFeedAccount::LEN])?;
            msg!("Initialized oracle feed: price={}", initial_price_micro);
        }
        OracleInstruction::SetPrice {
            price_micro,
            confidence_micro,
        } => {
            let mut data = feed_info.try_borrow_mut_data()?;
            let mut state = PriceFeedAccount::try_from_slice(&data[..PriceFeedAccount::LEN])
                .map_err(|_| ProgramError::InvalidAccountData)?;

            if state.magic != ORACLE_MAGIC {
                return Err(ProgramError::UninitializedAccount);
            }
            if state.authority != *authority_info.key {
                msg!("Signer does not match feed authority");
                return Err(ProgramError::IllegalOwner);
            }

            state.price_usd_micro = price_micro;
            state.confidence_micro = confidence_micro;
            state.last_updated_slot = clock.slot;
            state.last_updated_ts = clock.unix_timestamp;
            state.serialize(&mut &mut data[..PriceFeedAccount::LEN])?;
        }
    }

    Ok(())
}