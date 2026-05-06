# Sextant

[![Rust](https://img.shields.io/badge/Rust-1.95%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![License: LGPL-3.0](https://img.shields.io/badge/License-LGPL--3.0-blue.svg)](LICENSE)
[![NautilusTrader](https://img.shields.io/badge/NautilusTrader-fork-green)](https://github.com/nautechsystems/nautilus_trader)
[![Tests](https://img.shields.io/badge/tests-82%20passing-brightgreen)](#testing)

**AI-native quantitative trading engine.** Agents read market state from shared memory, output structured intents, engine compiles to optimal execution. The UI is a nautical dashboard — agents are crew, the market is the sea.

Built on a [NautilusTrader](https://github.com/nautechsystems/nautilus_trader) fork (tag-locked).

## Architecture

```
OKX WebSocket ──→ Nautilus Adapter ──→ QuoteTick
                                           │
                   ┌───────────────────────┤
                   ▼                       ▼
           ┌──────────────┐       ┌──────────────────┐
           │ StateEncoder │       │  Nautilus Core    │
           │ ContextWindow│       │  MessageBus       │
           │ mmap <50ns   │       │  ExecutionEngine  │
           └──────┬───────┘       │  RiskEngine       │
                  │               └────────▲──────────┘
                  ▼ mmap                   │
           ┌──────────────┐       ┌────────┴──────────┐
           │ Agent Swarm  │       │  IntentCompiler   │
           │  Perception  │──Intent──→  Limit/Market   │
           │  Momentum    │       │  TrailingStop     │
           │  MeanRev     │       │  TWAP/IOC/FOK     │
           │  Risk        │       └───────────────────┘
           └──────┬───────┘
                  │
           ┌──────┴───────┐
           │  Risk Field  │  logarithmic barrier, analytical gradient
           │  Autoresearch│  Karpathy Ratchet micro-backtest
           │  Reputation  │  ERC-8004 on-chain attestation
           └──────────────┘

Extended Events (mmap ring buffer)
  └──→ GUI (nautical dashboard)
  └──→ WebSocket (ws://localhost:8765/ws)
```

## Crates

| Crate | Description |
|-------|-------------|
| `agent_swarm` | Multi-agent trading: Agent trait, IntentCompiler, SwarmCoordinator, PerceptionRouter (L1 rules + L2/L3 LLM), TradeLimiter, Pipeline consensus |
| `state_encoder` | Shared-memory ContextWindow (seqlock double-buffer, <50ns) + ExtendedEventBuffer (256-slot mmap ring buffer) |
| `risk_potential` | Differentiable risk potential field `U(x) = -ln(1 - |x|/x_max)` with analytical gradients |
| `autoresearch` | Karpathy Ratchet: propose hypothesis → micro-backtest → accept/reject → ratchet never goes down |
| `reputation` | ERC-8004 on-chain attestation + AutonomySlider (Manual/Assisted/Auto) |
| `gui` | Nautical dashboard: sea chart, crew status bar, intent cards, hull integrity gauges |
| `live` | OKX live trading entry point (demo + prod), WebSocket event stream |

## Design Principles

| Principle | What It Means |
|-----------|---------------|
| **State-as-Context** | Engine state exposed as structured token flow in shared memory. Agents read via mmap, zero serialization, <50ns latency. |
| **Intent-Driven Execution** | Agents output structured intents, not raw orders. Engine compiles to optimal execution: Market, Limit (post-only), TrailingStop (bps offset), TWAP (sliced), IOC, FOK. |
| **Differentiable Risk** | Risk is a potential field, not a hard wall. Agents sense gradients and self-regulate. RiskAgent can veto any intent. |
| **Autoresearch Ratchet** | Agents propose hypotheses, micro-backtests validate, only improvements retained. Ratchet never goes down. |
| **Verifiable Trust** | Every decision attested on-chain (ERC-8004). Reputation score determines autonomy level and weighted consensus. |
| **95% Silent** | UI is a nautical dashboard — agents are crew. The system runs autonomously 95% of the time. Intent cards surface only when human approval is needed. |

## GUI — Nautical Dashboard

The GUI follows a "bridge viewport" paradigm, not a traditional quant terminal.

```
┌─────────────────────────────────────────────────────────────┐
│  ⚓ Sextant   BTC-USDT-SWAP   65,432.00   LONG 0.01   +12.50 │
├─────────────────────────────────────┬───────────────────────┤
│                                     │                       │
│           SEA CHART                 │   INTENT CARD         │
│    flowing price line +             │                       │
│    volatility heat overlay          │   🤖 momentum-01      │
│                                     │   "Buy BTC on trend"  │
│    color = volatility temp          │                       │
│    blue (calm) → red (storm)        │   [APPROVE] [REJECT]  │
│                                     │                       │
├─────────────────────────────────────┴───────────────────────┤
│  🟢 momentum  🟢 mean-rev  🟡 risk  🟢 perception │ [AUTO] [E-STOP] │
└─────────────────────────────────────────────────────────────┘
```

**Keyboard shortcuts:**
- `[1]` Orders · `[2]` Research · `[3]` Memory · `[4]` Hull Integrity
- `[Esc]` Close drawer

**Components:**
- **Sea Chart** — flowing price line with volatility heat overlay (not K-lines)
- **Crew Status** — 4 agent cells with status lights and one-line task descriptions
- **Intent Cards** — agent trade proposals with Approve/Modify/Reject buttons
- **Hull Integrity** — 6 circular gauges (Delta, Gamma, Vega, Liquidity, Concentration, Leverage)
- **Autonomy Slider** — Manual / Assisted / Auto
- **E-Stop** — one-click freeze all trading

See [docs/ui-design-v1.md](docs/ui-design-v1.md) for the full design specification.

## Quick Start

```bash
# Clone
git clone https://github.com/KuaaMU/sextant.git
cd sextant

# Test
cargo test --workspace

# Live trading (OKX demo, dry-run mode)
$env:SEXTANT_DRY_RUN=1; cargo run -p sextant-live

# GUI
cargo run -p sextant-gui -- --mmap sextant_context.mmap --events sextant_events.mmap
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OKX_API_KEY` | — | OKX API key |
| `OKX_API_SECRET` | — | OKX API secret |
| `OKX_API_PASSPHRASE` | — | OKX API passphrase |
| `OKX_ENVIRONMENT` | `demo` | `demo` or `live` |
| `SEXTANT_INSTRUMENT` | `BTC-USDT-SWAP.OKX` | Trading instrument |
| `SEXTANT_LEVERAGE` | `2` | Leverage multiplier |
| `SEXTANT_BASE_SIZE` | `1.0` | Order size (contracts) |
| `SEXTANT_MAX_TRADES` | `0` (unlimited) | Max trades (0 = unlimited for demo) |
| `SEXTANT_DRY_RUN` | `0` | `1` = log orders, don't submit |
| `SEXTANT_COOLDOWN_SECS` | `30` | Seconds between trading cycles |
| `SEXTANT_WS_PORT` | `8765` | WebSocket event stream port |
| `SEXTANT_EVENTS_PATH` | `sextant_events.mmap` | Extended event mmap file path |
| `OPENROUTER_API_KEY` | — | OpenRouter API key for LLM inference |

## Execution Styles

The IntentCompiler maps agent strategies to optimal execution:

| Agent Strategy | Execution Style | Nautilus Order |
|---------------|----------------|----------------|
| MomentumAgent (trend follow) | `TrailingStop { offset_bps: 50 }` | `trailing_stop_market()` |
| MeanReversionAgent (z-score fade) | `Limit { post_only: true }` | `limit()` |
| RiskAgent (veto) | Blocks all | No order |
| DeltaHedge | `Twap { slices: N, interval: D }` | `market()` per slice |
| Default | `Market` | `market()` |

## Testing

```bash
cargo test --workspace
# 82 tests passing across all crates

# Breakdown:
#   agent_swarm:    31 tests (compiler, swarm, risk agent, router, strategy)
#   state_encoder:  14 tests (context window, mmap, extended events, cross-process)
#   autoresearch:   10 tests (IR metrics, micro-backtest, ratchet)
#   risk_potential:  9 tests (gradient, barrier, position limits)
#   live:            8 tests (momentum, mean-reversion agents)
#   reputation:      5 tests (autonomy slider, attestation)
#   pipeline:        3 tests (full pipeline with router)
#   cross_process:   2 tests (mmap write/read consistency)
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

Currently verified on **OKX** (demo + live). All exchanges supported by NautilusTrader are available: Binance, Bybit, dYdX, Interactive Brokers, Betfair, Coinbase, Kraken, Deribit, Polymarket, Hyperliquid, and more.

## Project Structure

```
sextant/
├── Cargo.toml              # Workspace root
├── CLAUDE.md               # Development instructions
├── docs/
│   └── ui-design-v1.md     # GUI design specification
├── llm.toml                # LLM provider config (OpenRouter/vLLM)
└── crates/
    ├── agent_swarm/        # Multi-agent trading intelligence
    │   ├── src/intent.rs   # AgentIntent (unified contract)
    │   ├── src/compiler.rs # IntentCompiler (strategy → execution style)
    │   ├── src/strategy_wrapper.rs  # SwarmStrategy (Nautilus bridge)
    │   └── src/perception/ # L1 rules + L2/L3 LLM router
    ├── state_encoder/      # Shared-memory context bridge
    │   ├── src/context_window.rs    # ContextWindow (#[repr(C)], seqlock)
    │   └── src/extended_events.rs   # SextantEvent ring buffer
    ├── risk_potential/     # Differentiable risk field
    ├── autoresearch/       # Karpathy Ratchet engine
    ├── reputation/         # On-chain trust layer
    ├── gui/                # Nautical dashboard (egui)
    │   └── src/panels/     # sea_chart, crew_status, intent_card, pnl_strip, drawer
    └── live/               # OKX live trading + WebSocket event stream
        └── src/main.rs     # LiveNode + WS server at /ws
```

## Roadmap

| Phase | Timeline | Status | Focus |
|-------|----------|--------|-------|
| P0 | Now → Jun | **Done** | OKX demo, Nautilus internals, execution bridge |
| P1 | Jul → Aug | **Done** | StateEncoder, shared memory, LLM perception, TUI |
| P2 | Sep → Oct | **Done** | Multi-agent, autoresearch, reputation, risk gradient |
| P2.5 | May 2026 | **Done** | Execution bridge hardening, extended events, WS stream, GUI redesign |
| P3 | Nov → Dec | Frozen | (考研冲刺) |
| P4 | Jan+ | Planned | Autoresearch + Reputation production, on-chain attestation |

## License

LGPL-3.0-or-later
