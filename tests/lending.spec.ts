import * as anchor from "@coral-xyz/anchor";
import type { Program } from "@coral-xyz/anchor";
import BN from "bn.js";
import {
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";
import { expect } from "chai";

const ORACLE_PROGRAM_ID = new PublicKey("BSKYQxSA7STmDxYGqasHmb3Yqa5a6DeSGHtfs1oK58r9");
const ORACLE_ACCOUNT_LEN = 68;

function encodeOracleInit(priceMicro: bigint, confMicro: bigint): Buffer {
  const buf = Buffer.alloc(17);
  buf.writeUInt8(0, 0); // Variant 0 = Initialize
  buf.writeBigUInt64LE(priceMicro, 1);
  buf.writeBigUInt64LE(confMicro, 9);
  return buf;
}

function encodeOracleSetPrice(priceMicro: bigint, confMicro: bigint): Buffer {
  const buf = Buffer.alloc(17);
  buf.writeUInt8(1, 0); // Variant 1 = SetPrice
  buf.writeBigUInt64LE(priceMicro, 1);
  buf.writeBigUInt64LE(confMicro, 9);
  return buf;
}

describe("sol-lending-protocol e2e", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Lending as Program;
  const admin = (provider.wallet as anchor.Wallet).payer;
  const liquidator = Keypair.generate();

  let wsolMint: PublicKey;
  let usdcMint: PublicKey;
  const wsolOracle = Keypair.generate();
  const usdcOracle = Keypair.generate();

  let marketPda: PublicKey;
  let wsolReservePda: PublicKey;
  let usdcReservePda: PublicKey;
  let wsolVaultPda: PublicKey;
  let usdcVaultPda: PublicKey;
  let adminObligationPda: PublicKey;

  before(async () => {
    // Fund liquidator
    const sig = await provider.connection.requestAirdrop(
      liquidator.publicKey,
      5 * anchor.web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(sig);

    wsolMint = await createMint(provider.connection, admin, admin.publicKey, null, 6);
    usdcMint = await createMint(provider.connection, admin, admin.publicKey, null, 6);

    [marketPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("market")],
      program.programId
    );
    [wsolReservePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("reserve"), marketPda.toBuffer(), wsolMint.toBuffer()],
      program.programId
    );
    [usdcReservePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("reserve"), marketPda.toBuffer(), usdcMint.toBuffer()],
      program.programId
    );
    [wsolVaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), wsolReservePda.toBuffer()],
      program.programId
    );
    [usdcVaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), usdcReservePda.toBuffer()],
      program.programId
    );
    [adminObligationPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("obligation"), marketPda.toBuffer(), admin.publicKey.toBuffer()],
      program.programId
    );
  });

  it("initializes raw mock_oracle feeds ($100 SOL, $1 USDC)", async () => {
    const lamports = await provider.connection.getMinimumBalanceForRentExemption(ORACLE_ACCOUNT_LEN);

    for (const [oracleKp, price] of [
      [wsolOracle, 100_000_000n], // $100.00
      [usdcOracle, 1_000_000n],   // $1.00
    ] as const) {
      const tx = new Transaction().add(
        SystemProgram.createAccount({
          fromPubkey: admin.publicKey,
          newAccountPubkey: oracleKp.publicKey,
          lamports,
          space: ORACLE_ACCOUNT_LEN,
          programId: ORACLE_PROGRAM_ID,
        }),
        new TransactionInstruction({
          programId: ORACLE_PROGRAM_ID,
          keys: [
            { pubkey: oracleKp.publicKey, isSigner: false, isWritable: true },
            { pubkey: admin.publicKey, isSigner: true, isWritable: false },
          ],
          data: encodeOracleInit(price, 10_000n),
        })
      );
      await provider.sendAndConfirm(tx, [oracleKp]);
    }
  });

  it("initializes lending market and reserves", async () => {
    await program.methods
      .initMarket(5000) // 50% close factor
      .accounts({ admin: admin.publicKey })
      .rpc();

    // wSOL Reserve: 75% LTV, 80% Liq Threshold, 5% Bonus
    await program.methods
      .initReserve(7500, 8000, 500, 200, 1000)
      .accounts({
        market: marketPda,
        liquidityMint: wsolMint,
        oracle: wsolOracle.publicKey,
        admin: admin.publicKey,
      })
      .rpc();

    // USDC Reserve: 80% LTV, 85% Liq Threshold, 5% Bonus
    await program.methods
      .initReserve(8000, 8500, 500, 100, 800)
      .accounts({
        market: marketPda,
        liquidityMint: usdcMint,
        oracle: usdcOracle.publicKey,
        admin: admin.publicKey,
      })
      .rpc();
  });

  it("deposits collateral, borrows USDC, and liquidates when SOL drops to $70", async () => {
    const adminWsolAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      admin,
      wsolMint,
      admin.publicKey
    );
    const adminUsdcAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      admin,
      usdcMint,
      admin.publicKey
    );

    // Mint 10 wSOL to admin and 10,000 USDC liquidity into USDC reserve
    await mintTo(provider.connection, admin, wsolMint, adminWsolAta.address, admin, 10_000_000);
    await mintTo(provider.connection, admin, usdcMint, usdcVaultPda, admin, 10_000_000_000);

    await program.methods
      .initObligation()
      .accounts({ market: marketPda, owner: admin.publicKey })
      .rpc();

    // Deposit 10 wSOL ($1,000 value at $100/SOL)
    await program.methods
      .depositCollateral(new BN(10_000_000))
      .accounts({
        market: marketPda,
        reserve: wsolReservePda,
        obligation: adminObligationPda,
        liquidityVault: wsolVaultPda,
        userTokenAccount: adminWsolAta.address,
        owner: admin.publicKey,
      })
      .rpc();

    // Borrow 700 USDC (70% LTV <= 75% max LTV)
    await program.methods
      .borrow(new BN(700_000_000))
      .accounts({
        market: marketPda,
        collateralReserve: wsolReservePda,
        borrowReserve: usdcReservePda,
        obligation: adminObligationPda,
        borrowVault: usdcVaultPda,
        userReceiveTokenAccount: adminUsdcAta.address,
        collateralOracle: wsolOracle.publicKey,
        borrowOracle: usdcOracle.publicKey,
        owner: admin.publicKey,
      })
      .rpc();

    // Crash SOL price from $100 to $70 -> 10 SOL = $700 collateral.
    // Risk-adjusted collateral = $700 * 80% = $560 < $700 debt (Health Factor = 0.80)
    const crashTx = new Transaction().add(
      new TransactionInstruction({
        programId: ORACLE_PROGRAM_ID,
        keys: [
          { pubkey: wsolOracle.publicKey, isSigner: false, isWritable: true },
          { pubkey: admin.publicKey, isSigner: true, isWritable: false },
        ],
        data: encodeOracleSetPrice(70_000_000n, 10_000n),
      })
    );
    await provider.sendAndConfirm(crashTx);

    // Prepare liquidator token accounts
    const liqUsdcAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      admin,
      usdcMint,
      liquidator.publicKey
    );
    const liqWsolAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      admin,
      wsolMint,
      liquidator.publicKey
    );
    await mintTo(provider.connection, admin, usdcMint, liqUsdcAta.address, admin, 500_000_000);

    // Liquidate 300 USDC of debt
    await program.methods
      .liquidate(new BN(300_000_000))
      .accounts({
        market: marketPda,
        collateralReserve: wsolReservePda,
        borrowReserve: usdcReservePda,
        obligation: adminObligationPda,
        collateralVault: wsolVaultPda,
        borrowVault: usdcVaultPda,
        liquidatorDebtAccount: liqUsdcAta.address,
        liquidatorCollateralAccount: liqWsolAta.address,
        collateralOracle: wsolOracle.publicKey,
        borrowOracle: usdcOracle.publicKey,
        liquidator: liquidator.publicKey,
      })
      .signers([liquidator])
      .rpc();

    const updatedOb = await (
      program.account as unknown as {
        obligation: {
          fetch(address: PublicKey): Promise<{ depositedCollateral: anchor.BN }>;
        };
      }
    ).obligation.fetch(adminObligationPda);
    expect(updatedOb.depositedCollateral.toNumber()).to.be.lessThan(10_000_000);
  });
});