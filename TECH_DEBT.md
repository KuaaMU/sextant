# Sextant Technical Debt

## P0 Simplifications (current iteration)

| Location | Simplification | P1 Refactor Target | Trigger |
|----------|---------------|-------------------|---------|
| `swarm.rs:run_cycle` | Cycle-level 30s timeout only | Per-agent `AgentSandbox` with `catch_unwind` + resource quotas | >2 agents or third-party agents |
| `swarm.rs:resolve_conflicts` | NaN-safe `partial_cmp` tiebreak | `ConsensusEngine` trait with normalized confidence + z-score | >3 agents or dynamic weights |
| `compiler.rs:compile` | 3 basic `if` checks | `ValidationRule` chain for extensible validation | >5 validation rules |
| `router.rs:route` | Hash of `market_state[:200]` for cache key | `MarketStateKey` with semantic features (price bucket, trend, volatility) | Cache hit rate < 50% |
| `context_window.rs:is_stale` | Single `timestamp_ns` check | Per-field staleness (price TTL, position TTL, time TTL) | Different data sources with different freshness requirements |
| `mean_reversion_agent.rs` | `TradeLimiter` struct, no `force_trade` | Add `SEXTANT_FORCE_TRADE` support + `TradeLimiter` trait | Third agent added |
| `strategy_wrapper.rs:on_quote` | `block_in_place` + `block_on` | Dedicated swarm task with channel-based intent delivery | Performance profiling shows contention |

## P1 Architecture Upgrades

| Goal | Reference | Key Design |
|------|-----------|-----------|
| MessageBus integration | NautilusTrader | Immutable events, Pub/Sub pattern |
| Order state machine | NautilusTrader Events | `OrderEvent` enum, transition graph |
| Consensus RL | MAPPO paper | Shared reward, VDN architecture |
| Neural state encoder | TimesNet paper | Auto feature learning |
| Dry run mode | Polymarket Ghost Test | `SEXTANT_DRY_RUN` — real quotes, simulated execution |
| Independent risk task | HFT industry | Parallel `RiskEngine` with kill switch |

## P2 Frontier

| Goal | Reference | Key Design |
|------|-----------|-----------|
| Behavioral prediction | StockMARL | Observe other agent actions, not just prices |
| Risk-aware RL | OPHR paper | Volatility-specific agent architecture |
| Online learning | MAPPO | Continuous regime adaptation |
