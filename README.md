# Sextant

AI-native quantitative trading engine built on [NautilusTrader](https://github.com/nautechsystems/nautilus_trader).

Sextant replaces human-written strategies with autonomous Agent swarms that read market state from shared memory, output trading intents, and let the engine compile them into optimal execution plans.

## Architecture

```
Agent Swarm (Perception → Strategy → Risk → Execution)
        ↓ Intent
   Intent Compiler (Template Matching + Almgren-Chriss Optimization)
        ↓ ExecutionPlan
   Nautilus Core (MessageBus + ExecutionEngine + RiskEngine)
        ↓
   Exchange Adapter (Binance / OKX / Bybit / dYdX / IB / ...)
```

## Crates

| Crate | Description |
|-------|-------------|
| `agent_swarm` | Agent trait, IntentCompiler, SwarmCoordinator |
| `state_encoder` | Shared-memory ContextWindow with seqlock double-buffer |
| `risk_potential` | Differentiable risk potential field with analytical gradients |
| `autoresearch` | Karpathy Ratchet: micro-backtest + continuous strategy evolution |
| `reputation` | ERC-8004 on-chain attestation + autonomy level slider |
| `tui` | Terminal dashboard (ratatui) — 6 panels: Market, AgentLog, Risk, Orders, Research, Memory |

## Design Principles

- **State-as-Context**: Engine state exposed as structured token flow in shared memory. Agents read via mmap, zero serialization.
- **Intent-Driven Execution**: Agents output structured intents, not raw orders. Engine compiles to optimal execution (TWAP/VWAP/IOC).
- **Differentiable Risk**: Risk is a potential field, not a hard wall. Agents sense gradients and self-regulate.
- **Autoresearch Ratchet**: Agents propose hypotheses, micro-backtests validate, only improvements retained.
- **Verifiable Trust**: Every decision attested on-chain (ERC-8004). Reputation score determines autonomy level.

## Quick Start

```bash
# Clone
git clone https://github.com/KuaaMU/sextant.git
cd sextant

# Build
export PATH="$PATH:/c/Users/Administrator/.cargo/bin"
cargo check --workspace

# Test
cargo test --workspace
```

## Dependencies

Sextant depends on NautilusTrader via git (tag-locked):

```toml
nautilus-core = { git = "https://github.com/KuaaMU/nautilus_trader.git", tag = "sextant-base-v1.211.0" }
nautilus-model = { git = "https://github.com/KuaaMU/nautilus_trader.git", tag = "sextant-base-v1.211.0" }
nautilus-common = { git = "https://github.com/KuaaMU/nautilus_trader.git", tag = "sextant-base-v1.211.0" }
```

## Exchange Support

All exchanges supported by NautilusTrader are available:

Binance, OKX, Bybit, dYdX, Interactive Brokers, Betfair, Coinbase, Kraken, Deribit, Polymarket, Hyperliquid, and more.

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the full development plan.

| Phase | Timeline | Focus |
|-------|----------|-------|
| P0 | Now → Jun | Exchange demo, understand Nautilus internals |
| P1 | Jul → Aug | StateEncoder + shared memory + local LLM |
| P2 | Sep → Oct | AgentSwarm + IntentCompiler full loop |
| P3 | Jan+ | Autoresearch + Reputation |

## License

LGPL-3.0-or-later
