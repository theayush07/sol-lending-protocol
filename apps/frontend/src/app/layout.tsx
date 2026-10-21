// @ts-ignore -- CSS is loaded for its side effects by the Next.js bundler.
import "./global.css";
import React from "react";
import { SolanaProviders } from "../components/SolanaProviders";

declare global {
  namespace JSX {
    interface IntrinsicElements {
      [elemName: string]: any;
    }
  }
}

export const metadata = {
  title: "SolLend Protocol & Real-Time Analytics",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
   <html lang="en" suppressHydrationWarning>
      <body className="bg-slate-950 text-slate-100 antialiased">
        <SolanaProviders>{children}</SolanaProviders>
      </body>
    </html>
  );
}