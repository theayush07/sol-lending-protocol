-- CreateTable
CREATE TABLE "ProtocolEvent" (
    "id" TEXT NOT NULL,
    "signature" TEXT NOT NULL,
    "eventName" TEXT NOT NULL,
    "obligation" TEXT NOT NULL,
    "reserve" TEXT NOT NULL,
    "amount" BIGINT NOT NULL,
    "slot" BIGINT NOT NULL,
    "payload" JSONB NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "ProtocolEvent_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "ObligationPosition" (
    "obligationAddress" TEXT NOT NULL,
    "owner" TEXT NOT NULL,
    "collateralReserve" TEXT NOT NULL,
    "borrowReserve" TEXT NOT NULL,
    "depositedCollateral" BIGINT NOT NULL,
    "borrowedAmountWad" TEXT NOT NULL,
    "healthFactorBps" INTEGER NOT NULL,
    "updatedAtSlot" BIGINT NOT NULL,
    "updatedAt" TIMESTAMP(3) NOT NULL,

    CONSTRAINT "ObligationPosition_pkey" PRIMARY KEY ("obligationAddress")
);

-- CreateTable
CREATE TABLE "MarketMetricSnapshot" (
    "id" TEXT NOT NULL,
    "totalDepositsUsd" DOUBLE PRECISION NOT NULL,
    "totalBorrowsUsd" DOUBLE PRECISION NOT NULL,
    "utilizationRateBps" INTEGER NOT NULL,
    "liquidationsCount24h" INTEGER NOT NULL,
    "recordedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "MarketMetricSnapshot_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE INDEX "ProtocolEvent_obligation_idx" ON "ProtocolEvent"("obligation");

-- CreateIndex
CREATE INDEX "ProtocolEvent_eventName_createdAt_idx" ON "ProtocolEvent"("eventName", "createdAt");

-- CreateIndex
CREATE INDEX "ObligationPosition_healthFactorBps_idx" ON "ObligationPosition"("healthFactorBps");

-- CreateIndex
CREATE INDEX "ObligationPosition_owner_idx" ON "ObligationPosition"("owner");

-- CreateIndex
CREATE INDEX "MarketMetricSnapshot_recordedAt_idx" ON "MarketMetricSnapshot"("recordedAt");
