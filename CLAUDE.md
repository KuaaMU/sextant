# Sextant — Claude Code Instructions

## Project Overview
Sextant: AI-native quantitative trading engine.
NautilusTrader fork: `KuaaMU/nautilus_trader` (tag: `sextant-base-v1.211.1`)
Exchange: OKX (Demo Trading → Live).

## Architecture
Standalone workspace depending on NautilusTrader via git:
- `nautilus-core` — UUID4, UnixNanos
- `nautilus-model` — InstrumentId, QuoteTick, Price, Quantity
- `nautilus-common` — MessageBus subscribe API

## Crates
- `agent_swarm` — Agent trait, IntentCompiler, SwarmCoordinator
- `state_encoder` — Shared memory ContextWindow + seqlock
- `risk_potential` — Differentiable risk potential field
- `autoresearch` — Karpathy Ratchet micro-backtest
- `reputation` — ERC-8004 attestation + autonomy slider

## Development Rules
1. All new code in Sextant crates only — never modify Nautilus fork
2. Use `#[repr(C)]` for all shared-memory structures
3. Use `#[inline(always)]` for hot-path risk calculations
4. All new code must have unit tests
5. Use `async_trait` for Agent interface
6. Use `tracing` for logging, not `println!`

## Build Commands
```bash
export PATH="$PATH:/c/Users/Administrator/.cargo/bin"
cargo check --workspace
cargo test --workspace
cargo run -p sextant-live
```

## Logging
NautilusTrader's OKX adapter uses `log::info!()` / `log::debug!()` for diagnostics.
By default `stdout_level=Info` filters out debug messages. To see full adapter output:
- Set `stdout_level: LevelFilter::Debug` in `LoggerConfig`
- Set `use_tracing: true` for external crate tracing
- Or set `RUST_LOG=debug` env var

## Phases
- P0 (now-Jun): OKX demo, understand MessageBus/ExecutionEngine
- P1 (Jul-Aug): StateEncoder + shared memory + local LLM
- P2 (Sep-Oct): AgentSwarm + IntentCompiler
- P3 (Nov-Dec): FROZEN (考研)
- P4 (Jan+): Autoresearch + Reputation
