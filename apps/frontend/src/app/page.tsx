
"use client";
import dynamic from "next/dynamic";

const WalletMultiButton = dynamic(
  async () => (await import("@solana/wallet-adapter-react-ui")).WalletMultiButton,
  { ssr: false }
);

import React, { useEffect, useState } from "react";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
} from "recharts";

const INDEXER_API = process.env.NEXT_PUBLIC_INDEXER_URL || "http://127.0.0.1:4000";

export default function DashboardPage() {
  const [series, setSeries] = useState<any[]>([]);
  const [liquidations, setLiquidations] = useState<any[]>([]);
  const [underwater, setUnderwater] = useState<any[]>([]);

  useEffect(() => {
    const fetchMetrics = async () => {
      try {
        const [analyticsRes, liqRes] = await Promise.all([
          fetch(`${INDEXER_API}/api/analytics`),
          fetch(`${INDEXER_API}/api/liquidatable`),
        ]);
        if (analyticsRes.ok) {
          const data = await analyticsRes.json();
          setSeries(data.series || []);
          setLiquidations(data.recentLiquidations || []);
        }
        if (liqRes.ok) {
          const underData = await liqRes.json();
          setUnderwater(underData || []);
        }
      } catch {
        // Indexer offline or starting
      }
    };
    fetchMetrics();
    const timer = setInterval(fetchMetrics, 5000);
    return () => clearInterval(timer);
  }, []);

  const latest = series[series.length - 1] || {
    totalDepositsUsd: 0,
    totalBorrowsUsd: 0,
    utilizationRateBps: 0,
  };

  return (
    <main className="max-w-7xl mx-auto px-6 py-8 space-y-8">
      <header className="flex items-center justify-between border-b border-slate-800 pb-5">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">SolLend Protocol & Telemetry</h1>
          <p className="text-sm text-slate-400">
            Real-time Anchor event indexer, utilization telemetry, and liquidation monitor
          </p>
        </div>
        <WalletMultiButton />
      </header>

      <section className="grid grid-cols-1 md:grid-cols-3 gap-5">
        <div className="bg-slate-900 border border-slate-800 rounded-xl p-5">
          <span className="text-xs uppercase tracking-wider text-slate-400">Total Value Locked</span>
          <p className="text-3xl font-semibold mt-2 text-emerald-400">
            ${latest.totalDepositsUsd.toLocaleString()}
          </p>
        </div>
        <div className="bg-slate-900 border border-slate-800 rounded-xl p-5">
          <span className="text-xs uppercase tracking-wider text-slate-400">Total Borrowed</span>
          <p className="text-3xl font-semibold mt-2 text-sky-400">
            ${latest.totalBorrowsUsd.toLocaleString()}
          </p>
        </div>
        <div className="bg-slate-900 border border-slate-800 rounded-xl p-5">
          <span className="text-xs uppercase tracking-wider text-slate-400">Pool Utilization</span>
          <p className="text-3xl font-semibold mt-2 text-amber-400">
            {(latest.utilizationRateBps / 100).toFixed(2)}%
          </p>
        </div>
      </section>

      <section className="bg-slate-900 border border-slate-800 rounded-xl p-6">
        <h2 className="text-lg font-medium mb-4">Historical Deposits vs. Borrows (USD)</h2>
        <div className="h-72">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={series}>
              <CartesianGrid strokeDasharray="3 3" stroke="#1e293b" />
              <XAxis
                dataKey="recordedAt"
                tickFormatter={(v) => new Date(v).toLocaleTimeString()}
                stroke="#64748b"
              />
              <YAxis stroke="#64748b" />
              <Tooltip
                contentStyle={{ backgroundColor: "#0f172a", borderColor: "#334155" }}
              />
              <Area
                type="monotone"
                dataKey="totalDepositsUsd"
                stroke="#10b981"
                fill="#10b98133"
                name="Deposits (USD)"
              />
              <Area
                type="monotone"
                dataKey="totalBorrowsUsd"
                stroke="#38bdf8"
                fill="#38bdf833"
                name="Borrows (USD)"
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      </section>

      <section className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="bg-slate-900 border border-slate-800 rounded-xl p-6">
          <h2 className="text-lg font-medium mb-3">At-Risk / Underwater Positions</h2>
          <div className="overflow-x-auto">
            <table className="w-full text-sm text-left">
              <thead className="text-xs uppercase text-slate-400 border-b border-slate-800">
                <tr>
                  <th className="py-2">Obligation</th>
                  <th className="py-2">Collateral</th>
                  <th className="py-2">Health Factor</th>
                </tr>
              </thead>
              <tbody>
                {underwater.map((u) => (
                  <tr key={u.obligationAddress} className="border-b border-slate-800/60">
                    <td className="py-2.5 font-mono text-xs">
                      {u.obligationAddress.slice(0, 10)}...
                    </td>
                    <td className="py-2.5">{u.depositedCollateral}</td>
                    <td className="py-2.5 text-rose-400 font-semibold">
                      {(u.healthFactorBps / 10000).toFixed(3)}x
                    </td>
                  </tr>
                ))}
                {underwater.length === 0 && (
                  <tr>
                    <td colSpan={3} className="py-4 text-slate-500 text-center">
                      All indexed obligations are healthy (HF above 1.0x)
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>

        <div className="bg-slate-900 border border-slate-800 rounded-xl p-6">
          <h2 className="text-lg font-medium mb-3">Recent On-Chain Liquidations</h2>
          <div className="overflow-x-auto">
            <table className="w-full text-sm text-left">
              <thead className="text-xs uppercase text-slate-400 border-b border-slate-800">
                <tr>
                  <th className="py-2">Tx Signature</th>
                  <th className="py-2">Obligation</th>
                  <th className="py-2">Repaid</th>
                </tr>
              </thead>
              <tbody>
                {liquidations.map((ev) => (
                  <tr key={ev.id} className="border-b border-slate-800/60">
                    <td className="py-2.5 font-mono text-xs text-sky-400">
                      {ev.signature.slice(0, 12)}...
                    </td>
                    <td className="py-2.5 font-mono text-xs">
                      {ev.obligation.slice(0, 10)}...
                    </td>
                    <td className="py-2.5">{ev.amount}</td>
                  </tr>
                ))}
                {liquidations.length === 0 && (
                  <tr>
                    <td colSpan={3} className="py-4 text-slate-500 text-center">
                      No liquidation events indexed yet
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>
      </section>
    </main>
  );
}