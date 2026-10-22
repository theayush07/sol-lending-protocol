# SolLend

### A Solana-Based Decentralized Lending Protocol

SolLend is a full-stack decentralized finance (DeFi) lending protocol built on Solana using Rust and the Anchor framework. It explores the engineering challenges of on-chain lending, including collateralized borrowing, dynamic interest rate calculations, borrower health monitoring, automated liquidation workflows, and real-time protocol analytics.

The project integrates Solana programs with a TypeScript-based backend, a PostgreSQL event indexer, a liquidation monitoring service, and a Next.js dashboard to provide an end-to-end DeFi development environment.

> **Development status:** Experimental project intended for development and testing. The current setup uses a mock oracle for local development and is not intended for production use.

## Overview

SolLend is designed around the following components:

- **Lending protocol:** On-chain lending operations, collateral management, borrowing constraints, and liquidation logic.
- **Interest rate model:** Utilization-based borrowing interest calculations.
- **Oracle integration:** A custom Pyth-compatible mock oracle for development and testing.
- **Event indexer:** A TypeScript service for processing on-chain activity and storing analytics data.
- **Liquidation monitor:** A background service for identifying potentially liquidatable positions.
- **Web dashboard:** A Next.js interface for wallet connectivity and protocol interaction.
- **Observability infrastructure:** PostgreSQL, Redis, and Prometheus for data persistence and supporting monitoring infrastructure.

## Technology Stack

| Component | Technologies |
|---|---|
| Smart contracts | Rust, Anchor 0.30.1, Solana SBF |
| Token standards | Solana Program Library (SPL Token) |
| Oracle | Custom Pyth-compatible mock oracle |
| Backend services | Node.js, TypeScript |
| Database | PostgreSQL, Prisma ORM |
| Supporting infrastructure | Redis, Prometheus, Docker Compose |
| Frontend | Next.js, React, Tailwind CSS v3 |
| Wallet integration | Solana Wallet Adapter |
| Testing | Anchor, TypeScript, Mocha |

## Architecture

The protocol is organized into on-chain programs and off-chain services.

```mermaid
flowchart TB
    U[User] --> F[Next.js Frontend]
    F --> W[Solana Wallet]
    W --> L[Lending Program]

    O[Mock Oracle] --> L
    L --> E[On-chain Program Events]
    E --> I[TypeScript Event Indexer]
    I --> D[(PostgreSQL)]

    D --> A[Analytics API]
    A --> F

    L --> B[Liquidation Monitoring Bot]
    B --> L

    R[(Redis)] -. Supporting infrastructure .- I
    P[Prometheus] -. Metrics and monitoring .- I
```

### Component Responsibilities

| Component | Responsibility |
|---|---|
| `programs/lending` | Implements the core lending protocol and its on-chain rules. |
| `programs/mock-oracle` | Supplies mock price data for local development and testing. |
| `apps/indexer` | Processes program activity and maintains indexed protocol data. |
| `apps/bot` | Monitors borrower positions and evaluates liquidation eligibility. |
| `apps/frontend` | Provides the user interface and wallet-connected application experience. |
| `tests/` | Contains the Anchor-based integration test suite. |

The diagram illustrates the intended system design. Actual interactions and supported features depend on the implementation and configuration of each component.

## Repository Structure

```text
sol-lending-protocol/
├── apps/
│   ├── bot/                  # Liquidation monitoring service
│   ├── frontend/             # Next.js web application
│   └── indexer/              # Event indexer and analytics API
│
├── programs/
│   ├── lending/              # Core lending protocol
│   └── mock-oracle/          # Development price oracle
│
├── tests/                    # TypeScript integration tests
├── scripts/                  # Deployment and monitoring scripts
│
├── docker-compose.yml        # Local infrastructure configuration
├── Anchor.toml               # Anchor workspace configuration
├── Cargo.toml                # Rust workspace configuration
├── package.json              # Root package scripts
├── pnpm-workspace.yaml       # pnpm workspace configuration
└── README.md
```

*This structure assumes the repository contains the listed files and directories.*

## Getting Started

Follow these instructions to configure a local development environment.

### Prerequisites

Install the following tools:

- [Node.js](https://nodejs.org/) — version 18 or later
- [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/) and Cargo
- [Solana CLI](https://solana.com/docs/intro/installation)
- [Anchor](https://www.anchor-lang.com/docs/installation) — version 0.30.1
- [Docker Desktop](https://www.docker.com/products/docker-desktop/)
- Git

Verify your installations:

```bash
node --version
pnpm --version
rustc --version
cargo --version
solana --version
anchor --version
docker --version
```

Ensure that the Solana CLI, Anchor, and Rust toolchain are compatible with the versions expected by the project.

### 1. Clone the Repository

```bash
git clone https://github.com/theayush07/sol-lending-protocol.git
cd sol-lending-protocol
pnpm install
```

### 2. Start Infrastructure Services

Start PostgreSQL, Redis, and Prometheus:

```bash
docker compose up -d postgres redis prometheus
```

Verify the container status:

```bash
docker compose ps
```

The service names must match those defined in `docker-compose.yml`.

### 3. Start a Local Solana Validator

Open a separate terminal in the project directory:

```bash
solana config set --url localhost
solana-test-validator --reset
```

Keep the validator running for the remaining steps.

The `--reset` flag resets the local validator's ledger. Any previously deployed local programs and accounts will need to be recreated.

### 4. Build and Deploy the Programs

In another terminal:

```bash
anchor keys sync
anchor build
anchor deploy
```

Confirm that the Anchor provider is configured to use localnet and that the configured development wallet has sufficient SOL for deployment.

### 5. Configure the Indexer

Set the PostgreSQL connection string for local development:

```bash
export DATABASE_URL="postgresql://sollend:sollend_dev_password@localhost:5432/sollend_analytics?schema=public"
```

The credentials above are examples for the local environment. Adjust them to match your Docker Compose configuration.

Generate the Prisma client and apply the database migration:

```bash
pnpm --filter @sol-lend/indexer exec prisma generate
pnpm --filter @sol-lend/indexer exec prisma migrate dev --name init
```

Start the indexer:

```bash
pnpm dev:indexer
```

Keep the service running while testing the protocol and generating on-chain activity.

### 6. Run Integration Tests

With the local validator running and the programs deployed:

```bash
anchor test --skip-build --skip-local-validator --skip-deploy
```

This command assumes the programs have already been built and deployed and the Anchor test configuration is set up correctly.

Review the results to identify failed assertions, transaction errors, or missing account setup.

### 7. Start the Liquidation Monitoring Service

In a separate terminal:

```bash
export SIMULATION_ONLY="true"
pnpm dev:bot
```

Keep simulation mode enabled during local development. Verify that the bot's implementation respects this setting before testing any transaction-execution functionality.

### 8. Start the Frontend

Open another terminal:

```bash
pnpm dev:frontend
```

Navigate to:

**http://localhost:3000**

Connect a development wallet configured for localnet to interact with the locally deployed protocol.

> **Workspace note:** The commands above assume the package names and scripts exist in the repository. If a command fails, check the root `package.json`, `pnpm-workspace.yaml`, and the relevant workspace package configuration.

## Configuration

The following environment variables are used in the documented development workflow.

| Variable | Description | Development value |
|---|---|---|
| `DATABASE_URL` | PostgreSQL connection string | Local PostgreSQL URL |
| `SIMULATION_ONLY` | Enables the liquidation bot's simulation-only mode | `true` |

Additional variables may be required by the frontend, indexer, bot, or Solana programs.

For local development, consider using untracked environment files and documenting the required variable names in an `.env.example` file.

**Never commit private keys, seed phrases, wallet keypairs, or production credentials to version control.**

## Testing and Validation

The repository includes an Anchor-based TypeScript integration test suite.

Testing should cover the protocol behaviors implemented in the codebase, including:

- Collateral deposits and withdrawal constraints
- Borrowing limits and collateral validation
- Interest rate calculations
- Repayment behavior
- Borrower health and liquidation eligibility
- Invalid instructions and account validation
- Event emission and off-chain indexing

Build the programs before testing:

```bash
anchor build
```

Run the integration tests against an existing local validator and deployed programs:

```bash
anchor test --skip-build --skip-local-validator --skip-deploy
```

The scenarios listed above are recommended areas of validation, not a claim that every scenario is already covered by the current test suite.

## Security Considerations

SolLend is an experimental software project and must not be treated as a production lending protocol.

- **Mock price data:** The development oracle is not suitable for production financial operations.
- **Smart contract security:** No independent security audit is claimed.
- **Liquidation safety:** Oracle errors, incorrect account-health calculations, and transaction failures can affect liquidation outcomes.
- **Wallet security:** Use a dedicated development wallet with no valuable assets.
- **Bot execution:** Keep simulation mode enabled until transaction-execution behavior has been reviewed and tested.
- **Deployment:** Do not deploy with real assets without appropriate oracle integration, security reviews, risk controls, and comprehensive testing.

These safeguards are important because lending protocols handle collateral, debt positions, and risk-sensitive transactions.

## Development Roadmap

Potential areas for further development include:

- Integrating a production-appropriate oracle
- Expanding unit, integration, and invariant testing
- Strengthening liquidation execution and failure recovery
- Improving protocol analytics and historical activity visualizations
- Adding continuous integration for Rust and TypeScript validation
- Documenting configurable risk parameters and deployment procedures
- Reviewing contract security and protocol economics

These items represent possible improvements and are not guarantees of planned or completed functionality.

## Contributing

Contributions and technical feedback are welcome.

1. Fork the repository.
2. Create a feature branch.
3. Make your changes and add appropriate tests.
4. Run the relevant build and test commands.
5. Submit a pull request describing the implementation and its impact.

For changes involving lending rules, risk calculations, or liquidations, include tests for both expected behavior and failure cases.

## License

This project is licensed under the MIT License.
See the [LICENSE](LICENSE) file for details.
