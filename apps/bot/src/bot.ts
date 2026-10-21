import {
  ComputeBudgetProgram,
  Connection,
  Keypair,
  PublicKey,
  Transaction,
} from "@solana/web3.js";
import { AnchorProvider, BN, Idl, Program, Wallet } from "@coral-xyz/anchor";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import fs from "fs";
import path from "path";
import os from "os";

const logger = {
  info: (...args: unknown[]) => console.info("[liquidation-bot]", ...args),
  warn: (...args: unknown[]) => console.warn("[liquidation-bot]", ...args),
  error: (...args: unknown[]) => console.error("[liquidation-bot]", ...args),
};

const INDEXER_URL = process.env.INDEXER_URL || "http://127.0.0.1:4000";
const RPC_URL = process.env.RPC_URL || "http://127.0.0.1:8899";
const SIMULATION_ONLY = process.env.SIMULATION_ONLY !== "false"; // Defaults to safe simulation mode
const ALERT_WEBHOOK_URL = process.env.ALERT_WEBHOOK_URL || "";

/**
 * Abstraction for key management. In production, swap `LocalKeypairSigner`
 * with an AWS KMS / GCP KMS ed25519 signing implementation.
 */
interface SignerProvider {
  publicKey: PublicKey;
  getKeypair(): Keypair;
}

class LocalKeypairSigner implements SignerProvider {
  private kp: Keypair;
  constructor(keypairPath: string) {
    const raw = JSON.parse(fs.readFileSync(keypairPath, "utf8"));
    this.kp = Keypair.fromSecretKey(Uint8Array.from(raw));
  }
  get publicKey(): PublicKey {
    return this.kp.publicKey;
  }
  getKeypair(): Keypair {
    return this.kp;
  }
}

async function sendAlert(message: string) {
  if (!ALERT_WEBHOOK_URL) return;
  try {
    await fetch(ALERT_WEBHOOK_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ content: message }),
    });
  } catch (err) {
    logger.warn({ err }, "Webhook alert delivery failed");
  }
}

async function scanAndLiquidate(program: Program, connection: Connection, signer: SignerProvider) {
  const res = await fetch(`${INDEXER_URL}/api/liquidatable`);
  if (!res.ok) return;

  const targets = (await res.json()) as any[];
  if (!Array.isArray(targets) || targets.length === 0) return;

  logger.info({ count: targets.length }, "Found underwater obligations");

  const [marketPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("market")],
    program.programId
  );

  for (const target of targets) {
    try {
      const obligationPubkey = new PublicKey(target.obligationAddress);
      const colReservePubkey = new PublicKey(target.collateralReserve);
      const borReservePubkey = new PublicKey(target.borrowReserve);

      const colReserve: any = await (program.account as any).reserve.fetch(colReservePubkey);
      const borReserve: any = await (program.account as any).reserve.fetch(borReservePubkey);

      const debtTokens = BigInt(target.borrowedAmountWad) / 1000000000000000000n;
      const repayAmount = debtTokens / 2n; // 50% close factor
      if (repayAmount <= 0n) continue;

      const liquidatorDebtAta = getAssociatedTokenAddressSync(
        borReserve.liquidityMint,
        signer.publicKey
      );
      const liquidatorColAta = getAssociatedTokenAddressSync(
        colReserve.liquidityMint,
        signer.publicKey
      );

      const priorityFeeIx = ComputeBudgetProgram.setComputeUnitPrice({
        microLamports: 25_000,
      });

      const liquidateIx = await program.methods
        .liquidate(new BN(repayAmount.toString()))
        .accounts({
          market: marketPda,
          collateralReserve: colReservePubkey,
          borrowReserve: borReservePubkey,
          obligation: obligationPubkey,
          collateralVault: colReserve.liquidityVault,
          borrowVault: borReserve.liquidityVault,
          liquidatorDebtAccount: liquidatorDebtAta,
          liquidatorCollateralAccount: liquidatorColAta,
          collateralOracle: colReserve.oracle,
          borrowOracle: borReserve.oracle,
          liquidator: signer.publicKey,
        })
        .instruction();

      const tx = new Transaction().add(priorityFeeIx, liquidateIx);
      tx.feePayer = signer.publicKey;
      tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
      tx.sign(signer.getKeypair());

      // Always simulate before broadcasting
      const sim = await connection.simulateTransaction(tx);
      if (sim.value.err) {
        logger.warn(
          { obligation: target.obligationAddress, err: sim.value.err, logs: sim.value.logs },
          "Liquidation simulation reverted"
        );
        continue;
      }

      logger.info(
        {
          obligation: target.obligationAddress,
          unitsConsumed: sim.value.unitsConsumed,
          simulationOnly: SIMULATION_ONLY,
        },
        "Liquidation simulation succeeded"
      );

      if (!SIMULATION_ONLY) {
        const sig = await connection.sendRawTransaction(tx.serialize(), {
          skipPreflight: true,
        });
        await connection.confirmTransaction(sig, "confirmed");
        logger.info({ sig, obligation: target.obligationAddress }, "Executed live liquidation");
        await sendAlert(`Liquidated obligation ${target.obligationAddress} | tx: ${sig}`);
      }
    } catch (err) {
      logger.error({ err, obligation: target.obligationAddress }, "Error processing target");
    }
  }
}

async function main() {
  const keyPath =
    process.env.BOT_KEYPAIR_PATH || path.join(os.homedir(), ".config/solana/id.json");
  const signer = new LocalKeypairSigner(keyPath);
  const connection = new Connection(RPC_URL, "confirmed");
  const wallet = new Wallet(signer.getKeypair());
  const provider = new AnchorProvider(connection, wallet, { commitment: "confirmed" });

  const idlPath = path.resolve(__dirname, "../../../target/idl/lending.json");
  const idl: Idl = JSON.parse(fs.readFileSync(idlPath, "utf8"));
  const program = new Program(idl, provider);

  logger.info(
    { botPubkey: signer.publicKey.toBase58(), simulationOnly: SIMULATION_ONLY },
    "Liquidation bot started"
  );

  setInterval(() => {
    scanAndLiquidate(program, connection, signer).catch((err) =>
      logger.error(err, "Scan loop failed")
    );
  }, 5_000);
}

main();