"use client";

import React, { useMemo, useCallback } from "react";
import { ConnectionProvider, WalletProvider } from "@solana/wallet-adapter-react";
import { WalletModalProvider } from "@solana/wallet-adapter-react-ui";
import { WalletError } from "@solana/wallet-adapter-base";

import "@solana/wallet-adapter-react-ui/styles.css";

export function SolanaProviders({ children }: { children: React.ReactNode }) {
  const endpoint = process.env.NEXT_PUBLIC_RPC_URL || "http://127.0.0.1:8899";
  
  // Modern wallets auto-detect. We can leave this array empty to clear the Phantom warning.
  const wallets = useMemo(() => [], []);

  // Catch and silence the "User rejected the request" and "Not Ready" errors
  const onError = useCallback((error: WalletError) => {
    if (error.name === "WalletNotReadyError") {
      console.log("Wallet not ready. Please install a wallet extension.");
      return;
    }
    if (error.name === "WalletConnectionError") {
      console.log("Wallet connection was cancelled by the user.");
      return;
    }
    console.error("Wallet error:", error);
  }, []);

  return (
    <ConnectionProvider endpoint={endpoint}>
      {/* Removed autoConnect so it stops prompting you automatically on every page refresh */}
      <WalletProvider wallets={wallets} onError={onError}>
        <WalletModalProvider>{children}</WalletModalProvider>
      </WalletProvider>
    </ConnectionProvider>
  );
}