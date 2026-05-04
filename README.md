# Sextant

[![Rust](https://img.shields.io/badge/Rust-1.95%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![License: LGPL-3.0](https://img.shields.io/badge/License-LGPL--3.0-blue.svg)](LICENSE)
[![NautilusTrader](https://img.shields.io/badge/NautilusTrader-fork-green)](https://github.com/nautechsystems/nautilus_trader)

**AI-native quantitative trading engine.** Agents read market state from shared memory, output structured intents, engine compiles to optimal execution.

Built on [NautilusTrader](https://github.com/nautechsystems/nautilus_trader) — the same engine, zero fork modification.

## Architecture

```
Exchange WebSocket ──→ Nautilus Adapter ──→ QuoteTick
                                                │
                    ┌───────────────────────────┤
                    ▼                           ▼
            ┌──────────────┐          ┌──────────────────┐
            │ StateEncoder │          │   Nautilus Core   │
            │ ContextWindow│          │  MessageBus      │
            │ mmap <50ns   │          │  ExecutionEngine │
            └──────┬───────┘          │  RiskEngine      │
                   │                  └────────▲─────────┘
                   ▼ mmap                      │
            ┌──────────────┐          ┌────────┴─────────┐
            │  Agent Swarm │──Intent──→  IntentCompiler  │
            │  Perception  │          │  Template + AC   │
            │  Strategy    │          └──────────────────┘
            │  Risk        │
            └──────────────┘
                   │
            ┌──────┴───────┐
            │  Risk Field  │  ← logarithmic barrier, analytical gradient
            │  Autoresearch│  ← micro-backtest ratchet
            │  Reputation  │  ← ERC-8004 on-chain attestation
            └──────────────┘
```

## Crates

| Crate | Description |
|-------|-------------|
| `agent_swarm` | Agent trait, IntentCompiler, SwarmCoordinator — multi-agent trading intelligence |
| `state_encoder` | Shared-memory ContextWindow with seqlock double-buffer, zero-copy mmap bridge |
| `risk_potential` | Differentiable risk potential field with analytical gradients, `#[inline(always)]` |
| `autoresearch` | Karpathy Ratchet: micro-backtest + continuous strategy evolution |
| `reputation` | ERC-8004 on-chain attestation + autonomy level slider |
| `tui` | Terminal dashboard (ratatui) — 6 panels: Market, AgentLog, Risk, Orders, Research, Memory |
| `gui` | Desktop GUI interface |
| `live` | Live trading entry point |

## Design Principles

| Principle | What It Means |
|-----------|---------------|
| **State-as-Context** | Engine state exposed as structured token flow in shared memory. Agents read via mmap, zero serialization, <50ns latency. |
| **Intent-Driven Execution** | Agents output structured intents, not raw orders. Engine compiles to optimal execution (TWAP/VWAP/IOC). |
| **Differentiable Risk** | Risk is a potential field, not a hard wall. Logarithmic barrier `U(x) = -ln(1 - \|x\|/x_max)`, agents sense gradients and self-regulate. |
| **Autoresearch Ratchet** | Agents propose hypotheses, micro-backtests validate, only improvements retained. Ratchet never goes down. |
| **Verifiable Trust** | Every decision attested on-chain (ERC-8004). Reputation score determines autonomy level. |

## Quick Start

```bash
# Clone
git clone https://github.com/KuaaMU/sextant.git
cd sextant

# Build
cargo check --workspace

# Test
cargo test --workspace

# Run live trading (OKX demo)
cargo run -p sextant-live
```

## Dependencies

Sextant depends on a NautilusTrader fork via git (tag-locked):

```toml
nautilus-core       = { git = "https://github.com/KuaaMU/nautilus_trader.git", tag = "sextant-base-v1.211.2" }
nautilus-model      = { git = "https://github.com/KuaaMU/nautilus_trader.git", tag = "sextant-base-v1.211.2" }
nautilus-common     = { git = "https://github.com/KuaaMU/nautilus_trader.git", tag = "sextant-base-v1.211.2" }
nautilus-trading    = { git = "https://github.com/KuaaMU/nautilus_trader.git", tag = "sextant-base-v1.211.2" }
nautilus-execution  = { git = "https://github.com/KuaaMU/nautilus_trader.git", tag = "sextant-base-v1.211.2" }
nautilus-okx        = { git = "https://github.com/KuaaMU/nautilus_trader.git", tag = "sextant-base-v1.211.2" }
```

## Exchange Support

All exchanges supported by NautilusTrader are available:

Binance, OKX, Bybit, dYdX, Interactive Brokers, Betfair, Coinbase, Kraken, Deribit, Polymarket, Hyperliquid, and more.

## Roadmap

| Phase | Timeline | Focus |
|-------|----------|-------|
| P0 | Now → Jun | Exchange demo, understand Nautilus internals |
| P1 | Jul → Aug | StateEncoder + shared memory + local LLM |
| P2 | Sep → Oct | AgentSwarm + IntentCompiler full loop |
| P3 | Nov → Dec | Frozen (考研冲刺) |
| P4 | Jan+ | Autoresearch + Reputation |

See [ROADMAP.md](ROADMAP.md) for the full development plan and [BLUEPRINT.md](BLUEPRINT.md) for the technical design blueprint.

## Project Structure

```
sextant/
├── Cargo.toml          # Workspace root
├── CLAUDE.md           # Development instructions
├── ROADMAP.md          # Detailed development plan
├── BLUEPRINT.md        # Technical design blueprint (中文)
└── crates/
    ├── agent_swarm/    # Multi-agent trading intelligence
    ├── state_encoder/  # Shared-memory context bridge
    ├── risk_potential/ # Differentiable risk field
    ├── autoresearch/   # Strategy evolution engine
    ├── reputation/     # On-chain trust layer
    ├── tui/            # Terminal dashboard
    ├── gui/            # Desktop GUI
    └── live/           # Live trading entry point
```

## License

LGPL-3.0-or-later
