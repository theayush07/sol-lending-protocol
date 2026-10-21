import Fastify from "fastify";
import cors from "@fastify/cors";
import { Connection, PublicKey } from "@solana/web3.js";
import { BorshCoder, EventParser, Idl } from "@coral-xyz/anchor";
import { PrismaClient } from "@prisma/client";
import { Queue, Worker } from "bullmq";
import IORedis from "ioredis";
import pino from "pino";
import client from "prom-client";
import fs from "fs";
import path from "path";

const logger = pino({ level: process.env.LOG_LEVEL || "info" });
const prisma = new PrismaClient();
const redis = new IORedis(process.env.REDIS_URL || "redis://127.0.0.1:6379", {
  maxRetriesPerRequest: null,
});

// Prometheus metrics
const register = new client.Registry();
client.collectDefaultMetrics({ register });

const eventsIndexedCounter = new client.Counter({
  name: "sollend_events_indexed_total",
  help: "Total number of on-chain Anchor events indexed",
  labelNames: ["event_name"],
  registers: [register],
});

const RPC_URL = process.env.RPC_URL || "http://127.0.0.1:8899";
const PROGRAM_ID = new PublicKey(
  process.env.LENDING_PROGRAM_ID || "Lend111111111111111111111111111111111111111"
);

// Load generated Anchor IDL
const idlPath = path.resolve(__dirname, "../../../target/idl/lending.json");
const idl: Idl = JSON.parse(fs.readFileSync(idlPath, "utf8"));
const coder = new BorshCoder(idl);
const eventParser = new EventParser(PROGRAM_ID, coder);

async function startLogSubscriber(connection: Connection) {
  logger.info({ programId: PROGRAM_ID.toBase58(), rpc: RPC_URL }, "Starting Anchor log subscriber");

  connection.onLogs(
    PROGRAM_ID,
    async ({ logs, err, signature }) => {
      if (err) return;
      try {
        const events = eventParser.parseLogs(logs);
        for (const ev of events) {
          eventsIndexedCounter.inc({ event_name: ev.name });
          const data: any = ev.data;

          await prisma.protocolEvent.create({
            data: {
              signature,
              eventName: ev.name,
              obligation: data.obligation?.toBase58?.() ?? "unknown",
              reserve: (data.reserve ?? data.debtReserve)?.toBase58?.() ?? "unknown",
              amount: BigInt(data.amount?.toString?.() ?? data.repaidAmount?.toString?.() ?? "0"),
              slot: BigInt(data.slot?.toString?.() ?? "0"),
              payload: JSON.parse(JSON.stringify(data)),
            },
          });

          if (ev.name === "CollateralDeposited") {
            await prisma.obligationPosition.upsert({
              where: { obligationAddress: data.obligation.toBase58() },
              update: {
                depositedCollateral: BigInt(data.totalCollateral.toString()),
                updatedAtSlot: BigInt(data.slot.toString()),
              },
              create: {
                obligationAddress: data.obligation.toBase58(),
                owner: data.owner.toBase58(),
                collateralReserve: data.reserve.toBase58(),
                borrowReserve: "",
                depositedCollateral: BigInt(data.totalCollateral.toString()),
                borrowedAmountWad: "0",
                healthFactorBps: 99999,
                updatedAtSlot: BigInt(data.slot.toString()),
              },
            });
          } else if (ev.name === "LiquidityBorrowed") {
            await prisma.obligationPosition.update({
              where: { obligationAddress: data.obligation.toBase58() },
              data: {
                borrowReserve: data.reserve.toBase58(),
                borrowedAmountWad: data.totalBorrowedWad.toString(),
                healthFactorBps: Number(data.healthFactorBps.toString()),
                updatedAtSlot: BigInt(data.slot.toString()),
              },
            });
          } else if (ev.name === "PositionLiquidated") {
            await redis.del("hot:recent_liquidations");
          }
        }
      } catch (decodeErr) {
        logger.error({ err: decodeErr, signature }, "Failed to process log event");
      }
    },
    "confirmed"
  );
}

// BullMQ periodic time-series snapshot worker
const metricsQueue = new Queue("metrics-aggregation", { connection: redis });
new Worker(
  "metrics-aggregation",
  async () => {
    const positions = await prisma.obligationPosition.findMany();
    let totalCol = 0;
    let totalBor = 0;
    for (const p of positions) {
      totalCol += Number(p.depositedCollateral) / 1e6;
      totalBor += Number(BigInt(p.borrowedAmountWad) / 1000000000000000000n) / 1e6;
    }
    const utilBps = totalCol > 0 ? Math.min(10000, Math.round((totalBor / totalCol) * 10000)) : 0;
    const liqCount = await prisma.protocolEvent.count({
      where: { eventName: "PositionLiquidated" },
    });

    await prisma.marketMetricSnapshot.create({
      data: {
        totalDepositsUsd: totalCol * 100,
        totalBorrowsUsd: totalBor,
        utilizationRateBps: utilBps,
        liquidationsCount24h: liqCount,
      },
    });
    await redis.del("hot:analytics_series");
  },
  { connection: redis }
);

async function bootstrap() {
  const connection = new Connection(RPC_URL, "confirmed");
  await startLogSubscriber(connection);

  await metricsQueue.add(
    "rollup",
    {},
    { repeat: { every: 15_000 }, removeOnComplete: true }
  );

  const app = Fastify({ logger: false });
  await app.register(cors, { origin: "*" });

  app.get("/health", async () => ({ status: "ok", rpc: RPC_URL }));

  app.get("/metrics", async (_, reply) => {
    reply.header("Content-Type", register.contentType);
    return register.metrics();
  });

  app.get("/api/analytics", async () => {
    const cached = await redis.get("hot:analytics_series");
    if (cached) return JSON.parse(cached);

    const series = await prisma.marketMetricSnapshot.findMany({
      orderBy: { recordedAt: "asc" },
      take: 50,
    });
    const recentLiquidations = await prisma.protocolEvent.findMany({
      where: { eventName: "PositionLiquidated" },
      orderBy: { createdAt: "desc" },
      take: 15,
    });

    const response = {
      series,
      recentLiquidations: recentLiquidations.map((e) => ({
        ...e,
        amount: e.amount.toString(),
        slot: e.slot.toString(),
      })),
    };
    await redis.set("hot:analytics_series", JSON.stringify(response), "EX", 10);
    return response;
  });

  app.get("/api/liquidatable", async () => {
    const underwater = await prisma.obligationPosition.findMany({
      where: { healthFactorBps: { lt: 10000 } },
      orderBy: { healthFactorBps: "asc" },
      take: 25,
    });
    return underwater.map((u) => ({
      ...u,
      depositedCollateral: u.depositedCollateral.toString(),
      updatedAtSlot: u.updatedAtSlot.toString(),
    }));
  });

  const port = Number(process.env.PORT || 4000);
  await app.listen({ port, host: "0.0.0.0" });
  logger.info({ port }, "Indexer & Analytics API listening");
}

bootstrap().catch((err) => {
  logger.fatal(err, "Indexer crashed");
  process.exit(1);
});