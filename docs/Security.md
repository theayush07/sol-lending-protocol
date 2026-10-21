# Security Invariants & Pre-Audit Checklist

## 1. Arithmetic & Rounding Invariants
- **Overflow Protection**: Workspace `Cargo.toml` enforces `overflow-checks = true` in `[profile.release]`.
- **Directional Rounding**: `token_usd_value_micro` rounds collateral valuations DOWN (`round_up = false`) and debt valuations UP (`round_up = true`) so dust rounding cannot be exploited to drain reserve vaults.
- **WAD Scaling**: Borrow balances and interest indices are tracked in `10^18` WAD precision (`u128`) to prevent slot-by-slot truncation loss.

## 2. Oracle Safety
- **Staleness Guard**: `read_validated_oracle` rejects any price feed where `current_slot - last_updated_slot > 150`.
- **Confidence Ratio**: Rejects feeds where the 95% confidence interval exceeds 5% of the spot price (`confidence_micro * 20 <= price_usd_micro`).
- **Key Pinning**: Reserve PDA stores the canonical oracle `Pubkey`; `read_validated_oracle` enforces `require_keys_eq!`.

## 3. Account & CPI Validation
- **PDA Signer Seeds**: Vault authority is a PDA (`["vault", reserve.key()]`) with canonical bump stored on `Reserve`.
- **Stack Frame Budget**: Instruction account structs wrap `Reserve`, `Obligation`, and `TokenAccount` in `Box<Account<...>>` to stay strictly under Solana's 4,096-byte stack frame limit.
- **Multisig Upgrade Authority**: Before mainnet deployment, transfer program upgrade authority to a Squads v4 multisig (`solana program set-upgrade-authority <PROGRAM_ID> --new-upgrade-authority <SQUADS_VAULT>`).