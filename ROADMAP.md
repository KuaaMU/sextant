# Sextant — Development Roadmap

> AI-native quantitative trading engine built on NautilusTrader.
> Agents read market state from shared memory, output trading intents, engine compiles to optimal execution.

**Repository**: [KuaaMU/sextant](https://github.com/KuaaMU/sextant)
**Nautilus Fork**: [KuaaMU/nautilus_trader](https://github.com/KuaaMU/nautilus_trader) (tag: `sextant-base-v1.211.0`)
**Exchanges**: All NautilusTrader-supported — Binance, OKX, Bybit, dYdX, Interactive Brokers, Betfair, Coinbase, Kraken, Deribit, Polymarket, Hyperliquid, and more.

---

## Design Principles

| Pillar | Core Idea | Concrete Mechanism |
|--------|-----------|-------------------|
| **State-as-Context** | Engine state as structured token flow in shared memory | `ContextWindow` (`#[repr(C)]`) in `SharedStateBuffer`, mmap read <50ns, seqlock double-buffer |
| **Intent-Driven Execution** | Agents output intents, not raw orders | `AgentIntent` → `IntentCompiler` (template <1μs + Almgren-Chriss ~1ms) → `ExecutionDirective` |
| **Differentiable Risk** | Risk is a potential field, not a hard wall | Logarithmic barrier `U(x) = -ln(1-\|x\|/x_max)`, analytical gradients, `#[inline(always)]` |
| **Autoresearch Ratchet** | Built-in micro-backtest, only improvements retained | Hypothesis → 5min backtest → accept if IR > baseline×1.05, ratchet never goes down |
| **Verifiable Trust** | Every decision attested on-chain | ERC-8004 `Attestation` with SHA3-256, reputation score → autonomy level slider |

---

## System Architecture

```
Exchange WebSocket
      ↓ QuoteTick
┌─────────────────────────────────────────────────────────────────┐
│                    NAUTILUS CORE (fork, tag-locked)              │
│  MessageBus ──subscribe_quotes()──→ StateEncoder                │
│  ExecutionEngine ←──ExecutionDirective── IntentCompiler          │
│  RiskEngine (hard bottom line)                                   │
│  Trader ──add_strategy()──→ SwarmStrategy wrapper                │
└─────────────────────────────────────────────────────────────────┘
      ↓ QuoteTick callback                        ↑ AgentIntent
┌──────────────────────┐                  ┌────────────────────────┐
│    StateEncoder      │     mmap <50ns   │    AgentSwarm          │
│    ContextWindow     │──────────────────→    Agent::perceive()   │
│    SharedStateBuffer │                  │    SwarmCoordinator    │
│    seqlock double-buf│                  │    IntentCompiler      │
└──────────────────────┘                  └────────────────────────┘
      ↓ risk_potential field                          ↑ gradient feedback
┌──────────────────────┐                  ┌────────────────────────┐
│  RiskPotentialField  │──────────────────→    AutoresearchRuntime │
│  logarithmic barrier │                  │    MicroBacktestEngine │
│  analytical gradient │                  │    Ratchet loop        │
└──────────────────────┘                  └────────────────────────┘
                                                    ↓ attestation
                                          ┌────────────────────────┐
                                          │    ReputationClient    │
                                          │    ERC-8004 on-chain   │
                                          │    AutonomySlider      │
                                          └────────────────────────┘
```

---

## System Core

### StateEncoder — Shared Memory Context Window

**Purpose**: Bridge Nautilus engine state to agents via zero-copy shared memory.

#### ContextWindow Layout

`#[repr(C)]` fixed-size struct, zero heap allocation:

```
Offset  Field                Type              Bytes   Description
──────  ─────                ────              ─────   ───────────
0       version              u64               8       Seqlock counter, incremented each write
8       timestamp_ns         u64               8       UnixNanos from nautilus-core
16      instrument_id        [u8; 32]          32      UTF-8 instrument identifier
48      instrument_id_len    u8                1       Actual string length
49      market_state_len     u32               4       LLM-readable text length
53      market_state         [u8; 2048]        2048    "bid:150.20 | ask:150.30 | spread:0.10"
2101    position_size        f64               8       Current position (signed)
2109    entry_price          f64               8       Average entry price
2117    unrealized_pnl       f64               8       Mark-to-market PnL
2125    greeks               Greeks            32      delta/gamma/theta/vega, each f64
2157    risk_potential       f64               8       Total U(position, drawdown, concentration)
2165    position_potential   f64               8       U_position(p) gradient magnitude
2173    drawdown_potential   f64               8       U_drawdown(d) gradient magnitude
2181    event_trace_len      u32               4       Wrapping counter, event_count() = min(len, 64)
2185    event_trace          [EventToken; 64]  2048    Circular buffer, each token = 32 bytes
─────────────────────────────────────────────────────────────────
Total: ~4,233 bytes per ContextWindow
```

**EventToken**: `#[repr(C)]` — `event_type: u8`, `price: f64`, `size: f64`, `timestamp_ns: u64` (32 bytes).

#### SharedStateBuffer Seqlock Protocol

Double-buffer with atomic seqlock — lock-free reads, single-writer:

```
seq: AtomicU64
  bit 0     = writing flag (1 = write in progress)
  bit 1     = active buffer index (0 or 1)
  bits 2+   = version counter (incremented on each complete write)
```

**Write path** (engine side, single writer):
1. Toggle active buffer: `seq ^= 2` (flip bit 1)
2. Set writing flag: `seq |= 1`
3. Copy `ContextWindow` → `buffers[active_idx]`
4. Clear writing flag: `seq &= !1` (atomic store with Release)
5. Version auto-increments via step 4→1 cycle

**Read path** (agent side, lock-free):
1. Load `seq_start` (Acquire)
2. If bit 0 set → write in progress, retry
3. Read `buffers[seq_start & 2 != 0]`
4. Load `seq_end` (Acquire)
5. If `seq_start != seq_end` → torn read, retry (max 100 spins)
6. Return snapshot

**Performance**: write <100ns, read <50ns. Zero heap allocation. Zero syscalls after mmap setup.

#### StateEncoder Event Bridge

`encoder.rs` connects Nautilus events to ContextWindow:

| Method | Trigger | Action |
|--------|---------|--------|
| `on_quote(quote)` | MessageBus `subscribe_quotes()` callback | Format `market_state` as `"bid:{bid} \| ask:{ask} \| spread:{s}"`, push `EventToken{QUOTE}`, write to SharedStateBuffer |
| `update_position(size, entry, pnl)` | Position change event | Update position fields, recompute `risk_potential` via `RiskPotentialField` |
| `update_risk(total, pos, dd)` | Risk recalculation | Write risk potential + gradient magnitudes to ContextWindow |

**Integration**: StateEncoder registers as MessageBus subscriber via `subscribe_quotes()` — completely non-invasive to Nautilus fork. No fork modification required.

---

### AgentSwarm — Multi-Agent Trading Intelligence

**Purpose**: Replace human-written strategies with autonomous agents that perceive market state and output structured trading intents.

#### Agent Trait

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    fn id(&self) -> &str;
    async fn perceive(&mut self, ctx: &ContextWindow) -> AgentIntent;
    async fn on_feedback(&mut self, feedback: &AgentFeedback);
    fn confidence(&self) -> f64 { 0.5 }
}
```

- `perceive()`: reads ContextWindow (shared memory snapshot), returns `AgentIntent`
- `on_feedback()`: receives execution result (fill price, slippage, success/failure) for learning
- `confidence()`: self-assessed certainty [0.0, 1.0], used by SwarmCoordinator for conflict resolution

#### Three-Layer LLM Decision Routing

Inside `perceive()`, agents route decisions through three layers:

| Layer | Model | Latency | Coverage | Trigger |
|-------|-------|---------|----------|---------|
| Layer 1 | Rule engine (pattern match) | <1μs | 95% | Clear signals: MA crossover, spread normal, known regime |
| Layer 2 | Qwen3-8B (local) | ~50ms | 4% | Ambiguous: choppy regime, conflicting indicators |
| Layer 3 | Qwen3-27B (local) | ~500ms | 1% | Novel: flash crash, unprecedented spread, new instrument |

Routing decision is itself Layer 1 (rule-based) — negligible overhead. Layer 2/3 only invoked when rules are insufficient.

#### Intent Types

| IntentType | Description | Typical ExecutionStyle |
|------------|-------------|----------------------|
| `DeltaHedge` | Maintain delta-neutral position | `Twap { slices, interval }` |
| `GammaScalp` | Profit from gamma by trading around delta | `Ioc` (speed critical) |
| `TrendFollow` | Ride momentum with trailing stop | `TrailingStop { offset_bps }` |
| `MeanReversion` | Fade deviation from mean | `Limit { price, post_only }` |
| `LiquidationCapture` | Capture liquidation cascades | `Ioc` (speed critical) |
| `Hold` | Do nothing | (no orders generated) |

#### IntentCompiler — Two-Stage Pipeline

**Stage 1: Template Matching** (<1μs, deterministic):
```
IntentType::DeltaHedge          → ExecutionTemplate::DeltaHedgeTwap
IntentType::GammaScalp          → ExecutionTemplate::GammaScalpIoc
IntentType::TrendFollow         → ExecutionTemplate::TrendFollowTrailing
IntentType::MeanReversion       → ExecutionTemplate::MeanRevertLimit
IntentType::LiquidationCapture  → ExecutionTemplate::LiquidationCaptureIoc
IntentType::Hold                → ExecutionTemplate::Hold
```
Pure enum mapping, zero computation.

**Stage 2: Parameter Optimization** (~1ms):
- **TWAP slice count**: `optimal_slices(size, horizon)` using Almgren-Chriss simplified model:
  `slices = ceil(sqrt(size * volatility / (eta * horizon)))` where eta = market impact coefficient
- **Limit price**: parsed from ContextWindow mid-price +/- spread offset based on urgency
- **Trailing stop offset**: calibrated from recent volatility (ATR-based)
- **VWAP volume profile**: derived from `event_trace` historical volume distribution
- **Hard check**: `RiskPotentialField::hard_check(position, drawdown, concentration)` — rejects if over limit

#### SwarmCoordinator — Conflict Resolution

Collects intents from all agents in parallel (`tokio::join!`), then resolves conflicts:

| Strategy | Mechanism | Use Case |
|----------|-----------|----------|
| `Hierarchical { priority }` | Agents sorted by priority; first non-Hold intent wins; veto agents can block | Production default — risk agent has veto power |
| `WeightedVote` | Weight = `confidence * reputation_score`; majority wins | Democratic multi-strategy ensemble |
| `Pipeline` | Intents pass through agents sequentially; each can modify or veto | Chain-of-thought reasoning pipeline |

Output: deduplicated `Vec<ExecutionDirective>` passed to Nautilus ExecutionEngine.

#### SwarmStrategy Wrapper

Implements Nautilus `Strategy` trait to bridge AgentSwarm into the engine:
- `on_quote()` → reads ContextWindow from SharedStateBuffer → calls `SwarmCoordinator::run_cycle()` → submits orders via ExecutionEngine
- Registered via `trader.add_strategy()` — completely non-invasive, zero Nautilus fork modification

---

### RiskPotentialField — Differentiable Risk Management

**Purpose**: Replace hard-coded risk rules with a continuous potential field that agents can sense and react to.

#### Mathematical Foundation

Logarithmic barrier functions — approach +infinity at limits:

```
U_position(p)      = -ln(1 - |p| / p_max)         -> +inf as |p| -> p_max
U_drawdown(d)      = -ln(1 - d / d_max)            -> +inf as d  -> d_max
U_concentration(w) = -ln(1 - w / w_max)            -> +inf as w  -> w_max

U_total = U_position(p) + U_drawdown(d) + U_concentration(w)
```

#### Analytical Gradients

No numerical differentiation — exact analytical derivatives:

```
dU_position/dp      = sign(p) / (p_max - |p|)
dU_drawdown/dd      = 1 / (d_max - d)
dU_concentration/dw = 1 / (w_max - w)

RiskGradient { position, drawdown, concentration }
gradient.magnitude() = sqrt(pos^2 + dd^2 + conc^2)   // L2 norm
```

#### Performance Guarantees

- All functions `#[inline(always)]` — zero function call overhead
- All parameters and returns `f64` — zero heap allocation
- Single gradient computation: ~2ns (one division + branch)
- Used on every tick in hot path — designed for zero overhead

#### Two Enforcement Layers

| Layer | Mechanism | When | Bypassable? |
|-------|-----------|------|-------------|
| **Soft** | Gradient feedback in ContextWindow | Every tick | Yes — agent self-regulates (99% of cases) |
| **Hard** | `hard_check()` in IntentCompiler | Order submission | No — blocks orders exceeding limits (flash crash protection) |

**Integration**: `StateEncoder::update_risk()` calls `RiskPotentialField::total()` and `gradient()`, writes values to ContextWindow fields. Agents read gradient magnitude in `perceive()` — if too close to wall, they reduce position or switch to `Hold`.

---

### Autoresearch — Karpathy Ratchet Evolution

**Purpose**: Built-in strategy evolution loop — propose, test, retain only improvements.

#### Ratchet Loop

```
+--------------+     +--------------+     +--------------+
|  Hypothesis  |---->| Micro-Backtest|---->|  Ratchet     |
|  Generation  |     | (5 min window)|     |  Decision    |
|              |     |               |     |              |
| Agent or LLM |     | Replay on     |     | IR_candidate |
| proposes     |     | historical    |     | > IR_baseline|
| code patch   |     | returns       |     | x 1.05?      |
+--------------+     +--------------+     +------+-------+
                                                  |
                                    +-------------+-------------+
                                    v YES         v NO
                              +----------+  +----------+
                              | Accept:  |  | Reject:  |
                              | update   |  | discard +|
                              | baseline |  | record   |
                              | IR       |  | reason   |
                              +----------+  +----------+
```

Key invariant: **ratchet only goes up**. Baseline IR is only updated when a candidate strictly beats it by >5%.

#### RiskAdjustedInfoRatio

```rust
IR = mean(returns) / std(returns)
IR_risk_adjusted = mean(returns) / sqrt(var(returns) + mean(risk_potentials))
is_improvement(candidate, baseline, threshold=0.05) = candidate > baseline * 1.05
```

#### MicroBacktestEngine

- **Window**: 5 minutes of market data (configurable `window_ns`)
- `run(returns)`: computes `total_return`, `max_drawdown`, `trade_count`, `ir`
- `run_delta(baseline, candidate)`: computes `FillDiff` per timestamp showing divergence points + `ir_delta`
- **Incremental replay**: only replays timestamps where candidate diverges >10% from baseline → ~10x speedup

#### AutoresearchRuntime

- `submit(hypothesis)`: queues for next ratchet cycle
- `run_ratchet(baseline_returns)`: evaluates ALL queued hypotheses
  - Best candidate that beats baseline → accepted, baseline updated
  - All others → rejected with recorded reason
- `history: Vec<HypothesisRecord>`: full audit trail — hypothesis, result, accepted/rejected, timestamp

---

### Reputation — ERC-8004 On-Chain Attestation

**Purpose**: Verifiable trust layer — every trading decision attested on-chain, reputation determines autonomy.

#### Attestation

```rust
struct Attestation {
    agent_id: UUID4,              // from nautilus-core
    decision_hash: [u8; 32],     // SHA3-256 of the AgentIntent
    outcome: TradeOutcome,        // realized_pnl, max_adverse_excursion, slippage_bps, target_hit
    timestamp_ns: u64,
    stake_amount: u128,           // economic stake backing this attestation
}
// hash() = SHA3-256 of entire struct -> posted on-chain
```

#### AutonomyLevel Slider

| Level | Reputation Score | Max Trade Size | Requires Confirmation |
|-------|-----------------|----------------|----------------------|
| Frozen | <=30 | $0 | Always (disabled) |
| Low | 30-50 | $1,000 | Always |
| Medium | 50-70 | $10,000 | >$5,000 |
| High | 70-90 | $100,000 | >$50,000 |
| Full | >90 | Unlimited | Never |

- **Upgrade-only**: `AutonomySlider::update()` only increases level (moving average of recent scores)
- **Force downgrade**: `force_level()` for manual override (risk agent veto, emergency stop)
- **Minimum observations**: requires N data points before first upgrade (prevents cold-start gaming)

#### ReputationClient

- Supports Ethereum and Solana chains
- `attest()`: submit Attestation on-chain (testnet stub -> mainnet integration)
- `query_score()`: read reputation score from on-chain registry
- `autonomy_level()`: convenience method combining score query + level calculation

---

### Full Data Path — End-to-End Flow

```
Exchange WebSocket -> Nautilus Adapter -> QuoteTick
    |
    v
MessageBus.publish("data.quotes.{instrument}", quote)
    |
    v
StateEncoder.on_quote(quote)  [via msgbus.subscribe_quotes()]
    |-> ContextWindow.market_state = "bid:{bid} | ask:{ask} | spread:{s}"
    |-> ContextWindow.push_event(EventToken{QUOTE, price, size, timestamp})
    |-> RiskPotentialField.total(position, drawdown, concentration)
    |   -> ContextWindow.risk_potential
    |-> RiskPotentialField.gradient(...)
    |   -> ContextWindow.position_potential, drawdown_potential
    +-> SharedStateBuffer.write(&ctx)  [seqlock, <100ns]
            | mmap [lock-free, <50ns]
    v
    Agent::perceive(&ContextWindow)
    |-> Layer 1 rule engine: pattern match -> IntentType
    |-> Layer 2 (4%): Qwen3-8B -> IntentType
    +-> Layer 3 (1%): Qwen3-27B -> IntentType + hypothesis
            | AgentIntent
    v
    SwarmCoordinator.resolve_conflicts(intents)
    |-> Hierarchical: first non-Hold by priority, risk agent veto
    |-> WeightedVote: confidence x reputation weighted majority
    +-> Pipeline: sequential pass-through with veto
            | AgentIntent (deduplicated)
    v
    IntentCompiler.compile(intent, ctx)
    |-> Stage 1: from_intent() -> ExecutionTemplate [<1us]
    |-> Stage 2: optimal_slices(), parse_mid_price() [~1ms]
    +-> hard_check() -> reject if over limit
            | ExecutionDirective
    v
    Nautilus ExecutionEngine.submit_order(orders)
    |-> RiskEngine.check_order()  [Nautilus hard bottom line]
    +-> Exchange Adapter -> WebSocket -> Exchange API
            | fill event
    v
    AgentFeedback{success, fill_price, fill_quantity, slippage_bps}
    |-> Agent::on_feedback(feedback)  [learn]
    |-> Attestation -> ReputationClient.attest()  [on-chain]
    +-> AutoresearchRuntime.submit(hypothesis)  [if improvement proposed]
```

---

## TUI — Terminal Dashboard

Real-time terminal monitoring for the Sextant engine. Inspired by [zenith](https://github.com/bvaisvil/zenith), [btm](https://github.com/ClementTsang/bottom), [k9s](https://github.com/derailed/k9s), [tickrs](https://github.com/tarkah/tickrs), [cointop](https://github.com/cointop-sh/cointop), [sampler](https://github.com/sqshq/sampler).

### Architecture

```
+---------------------------------------------------------+
|                   TUI Process (ratatui)                  |
|                                                         |
|  +--------------+  +--------------+  +--------------+   |
|  |  Event Loop  |  |  Data Layer  |  |  Panel Layer |   |
|  |  crossterm   |->|              |->|              |   |
|  |  16ms tick   |  | mmap_reader  |  | market.rs    |   |
|  |  key events  |  | jsonl_tail   |  | agent_log.rs |   |
|  |              |  | state_file   |  | risk.rs      |   |
|  +--------------+  +--------------+  | orders.rs    |   |
|                                      | research.rs  |   |
|                                      | memory.rs    |   |
|                                      +--------------+   |
+---------------------------------------------------------+
          | mmap                  | fs notify
  +---------------+      +---------------+
  | Sextant Engine|      | decisions.jsonl|
  | (writes shm)  |      | (decision log) |
  +---------------+      +---------------+
```

- **Event-driven refresh**: mmap version change -> repaint corresponding panel
- **16ms tick** (~60fps): even without new data, check every 16ms
- **Space to freeze**: pause rendering, data continues collecting, unfreeze to catch up

### 6 Panels

#### [1] Market — Real-Time Price + Order Book

**Data source**: SharedStateBuffer mmap + seqlock

```
+-- [1] Market -------------------------------------------------------+
| SOL-USDC @ 150.23  ^ +2.3%                                          |
|                                                                       |
| 152.00 -                          +--+                                |
| 151.00 -                      +--+  +--+                              |
| 150.23 -- -- -- -- -- -- -- --+-- -- -- +--+-- current                |
| 149.00 -              +----+--                                       |
| 148.00 -------------+--                                               |
|        +------------------------------------------------+             |
|        10:00   10:15   10:30   10:45   11:00   11:15                  |
|                                                                       |
| Order Book                    |  Recent Trades                       |
| ----------------------------- | ---------------------                |
|  Price    Size     Total      |  Time    Price   Size   Side         |
|  150.50   12.5    ########   |  11:14   150.23  0.5   BUY          |
|  150.40   8.3     #####...   |  11:14   150.20  1.2   SELL         |
|  150.30   45.2    #########  |  11:13   150.25  0.3   BUY          |
| -------- best ask ---------- |  11:13   150.30  2.0   SELL         |
|  150.20   23.1    #########  |  11:13   150.22  0.8   BUY          |
|  150.10   67.4    ########## |  11:12   150.15  1.5   BUY          |
|  150.00   120.0   ########## |                                      |
| -------- best bid ---------- |                                      |
|                                                                       |
| [1m] [5m] [15m] [1h]  <->:timeline  +/-:zoom  j/k:instrument        |
+-----------------------------------------------------------------------+
```

| Key | Action |
|-----|--------|
| `h/l` | Pan time axis left/right |
| `+/-` | Zoom in/out |
| `j/k` | Switch instrument |
| `1-4` | Switch timeframe (1m/5m/15m/1h) |
| `o` | Toggle order book visibility |

**Reference**: tickrs (candlestick rendering), zenith (line charts), trippy (time-series)

#### [2] Agent Log — Decision Flow

**Data source**: JSONL decision log file (tokio::fs + notify)

```
+-- [2] Agent Log -----------------------------------------------------+
| v 11:14:23 PERCEPTION  12ms  regime:trend  confidence:0.82            |
|   +-- input:  ContextWindow{SOL-USDC, bid:150.20, ask:150.30}        |
|   +-- output: regime=TrendUp, volatility=0.023                        |
|                                                                       |
| v 11:14:23 STRATEGY     8ms  intent:TrendFollow  conf:0.75           |
|   +-- target: +50 SOL @ market                                        |
|   +-- risk_budget: max_loss=$500, max_dd=50bps                        |
|                                                                       |
| v 11:14:23 RISK         1ms  gradient_mag:0.23  pass                  |
|   +-- potential: 0.45 -> 0.52 (increasing, but within bounds)         |
|                                                                       |
| v 11:14:23 EXECUTION    3ms  style:Twap  slices:5                     |
|   +-- order: BUY 10 SOL @ market  ->  fill: 150.25  slip: 2.1bps     |
|                                                                       |
| Filter: [ALL] PERCEPTION STRATEGY RISK EXEC  /:search  f:follow       |
+-----------------------------------------------------------------------+
```

| Key | Action |
|-----|--------|
| `j/k` | Navigate entries |
| `Enter` | Expand/collapse decision block |
| `Tab` | Cycle filter by agent type |
| `/` | Search decision content |
| `f` | Follow mode (auto-scroll) |
| `e` | Export view as JSON |

**Reference**: Warp (block quote folding), Chrome DevTools Network (timing bars), k9s (filter highlights)

#### [3] Risk — Potential Field + Greeks

**Data source**: SharedStateBuffer mmap (risk_potential, greeks fields)

```
+-- [3] Risk ----------------------------------------------------------+
| SOL-USDC  Risk Potential: 0.52  |  Gradient: 0.23 (moderate)         |
|                                                                       |
| Position Potential    Drawdown Potential    Concentration             |
| +-----------------+  +-----------------+  +-----------------+        |
| | U(p) = 0.45     |  | U(d) = 0.08     |  | U(w) = 0.31     |        |
| | ########........ |  | ##.............. |  | ######.......... |        |
| | 45% of limit    |  | 8% of limit     |  | 31% of limit    |        |
| +-----------------+  +-----------------+  +-----------------+        |
|                                                                       |
| Risk Potential Over Time                                              |
| 1.0 ---- -- -- -- -- -- -- -- -- -- -- -- -- hard limit -- -- --     |
| 0.8                                                                  |
| 0.6                            +--+                                   |
| 0.4                       +--+    +--+                                |
| 0.2 ----------------+--+            +--+ current                     |
| 0.0 ----------------+--                                               |
|     +------------------------------------------------+                |
|                                                                       |
| Greeks Matrix                                                         |
| +----------+----------+----------+----------+----------+              |
| |          |  Delta   |  Gamma   |  Theta   |  Vega    |              |
| +----------+----------+----------+----------+----------+              |
| | SOL-USDC |  +0.45   |  0.012   |  -23.5   |  +12.3   |              |
| | ETH-USDC |  -0.30   |  0.008   |  -15.2   |  +8.7    |              |
| | BTC-USDC |  +0.15   |  0.003   |   -8.1   |  +5.2    |              |
| +----------+----------+----------+----------+----------+              |
|                                                                       |
| j/k:instrument  g:Greeks  p:potential  !:emergency close              |
+-----------------------------------------------------------------------+
```

| Key | Action |
|-----|--------|
| `j/k` | Switch instrument |
| `g` | Toggle Greeks matrix view |
| `p` | Toggle potential field detail view |
| `!` | Emergency close position (requires confirmation) |
| `t` | Switch timeframe |

**Reference**: zenith (real-time curves), nvtop (split-pane), btm (dashboard gauges)

#### [4] Orders — Execution History

**Data source**: Execution events from Nautilus ExecutionEngine

```
+-- [4] Orders --------------------------------------------------------+
| ID            Instrument  Side  Qty    Price    Status    Slippage    |
| -------------------------------------------------------------------- |
| #001  SOL-USDC   BUY    50.0  market   o FILL     2.1bps            |
| #002  SOL-USDC   BUY    30.0  market   O PARTIAL  3.5bps            |
| #003  ETH-USDC   SELL   5.0   3200.00  o ACK     -                  |
| #004  SOL-USDC   BUY    20.0  market   O PENDING  -                  |
| #005  BTC-USDC   BUY    0.1   market   X DENY    -                  |
|                                                                       |
| -- Selected: #002 --------------------------------------------------  |
| Intent: TrendFollow SOL-USDC +50 -> Compiler: Twap 5 slices          |
| Fill: 30/50 @ avg 150.25  |  Slippage: 3.5bps  |  Agent: trend-01   |
| Feedback: AgentFeedback{success=true, fill_price=150.25}              |
|                                                                       |
| Colors: o FILL=green  O PARTIAL=cyan  o ACK=blue                     |
|         O PENDING=yellow  X DENY=red                                  |
|                                                                       |
| j/k:navigate  Enter:detail  s:sort  f:filter  /:search               |
+-----------------------------------------------------------------------+
```

| Key | Action |
|-----|--------|
| `j/k` | Navigate orders |
| `Enter` | Drill-down: Intent -> Compiler -> Order -> Fill full chain |
| `s` | Sort column (time/amount/slippage) |
| `f` | Filter by status (All/Filled/Partial/Pending/Denied) |
| `/` | Search order ID or instrument |
| `c` | Cancel selected order |

**Reference**: k9s (list->detail drill-down), htop (sortable table), lazygit (status color coding)

#### [5] Research — Autoresearch Ratchet

**Data source**: AutoresearchRuntime state file + history

```
+-- [5] Research ------------------------------------------------------+
| Ratchet Status: Baseline IR = 1.23  |  Accepted: 12  Rejected: 38   |
|                                                                       |
| IR Evolution (risk-adjusted information ratio)                        |
| 1.4                                                +-- accepted      |
| 1.3                                           +----+                 |
| 1.2                    +----------------+--+                          |
| 1.1               +--+              +-+                               |
| 1.0 ------------+--                                                    |
|     +------------------------------------------------+                |
|      round 0    25    50    75   100   125   150                      |
|                                                                       |
| Hypothesis Scatter Plot                                               |
|     IR ^                                                              |
|  1.5 |              o  o                                              |
|  1.3 |         o  o o  o  o  <- accepted (green)                      |
|  1.2 |-- -- -- -- -- -- -- -- baseline -- -- --                       |
|  1.0 |     x  x  x  x  x  x  <- rejected (red)                       |
|  0.8 |  x  x  x     x                                                |
|      +--------------------------------------------> complexity         |
|                                                                       |
| -- Latest Hypotheses ------------------------------------------------ |
| v #H-047  "Increase momentum window to 20 bars"  o ACCEPTED          |
|   IR: 1.32 -> 1.38 (+4.6%)  |  Parent: #H-031                        |
|   + window = 15 -> 20                                                  |
|   + threshold = 0.02 -> 0.015                                         |
|                                                                       |
| > #H-048  "Add volume filter"  X REJECTED                            |
|   IR: 1.38 -> 1.35 (-2.2%)  |  Parent: #H-047                        |
|                                                                       |
| j/k:navigate  Enter:expand diff  r:trigger ratchet  h:history         |
+-----------------------------------------------------------------------+
```

| Key | Action |
|-----|--------|
| `j/k` | Navigate hypothesis list |
| `Enter` | Expand hypothesis code diff |
| `r` | Manually trigger one Ratchet cycle |
| `h` | View full hypothesis history |
| `p` | Pause/resume Autoresearch |

**Reference**: tickrs (scatter plots), k9s (drill-down), lazygit (folded details)

#### [6] Memory — Performance Monitor

**Data source**: perf counters + mmap seqlock version

```
+-- [6] Memory --------------------------------------------------------+
| Shared Memory Status                      Seqlock Monitor             |
| +------------------------------------+  Version: 1,247,893           |
| | Write Latency Distribution (ns)    |  Writes/s: 12,450             |
| |                                    |  Reads/s:  8,230              |
| |  50ns  ####################  85%   |  Torn Reads: 0                |
| | 100ns  ########............  12%   |                               |
| | 200ns  ##..................   2%   |  Lock Contention: 0.01%      |
| | 500ns  #...................   1%   |                               |
| +------------------------------------+                               |
|                                                                       |
| Read Latency Sparkline (last 60s)                                    |
| 80ns - +--+  +--+                                                     |
| 60ns -+    +--+    +--+  +--+                                         |
| 40ns -+        +----+  +-------------------- avg: 47ns                |
|      +------------------------------------------------+                |
|                                                                       |
| ContextWindow Size: 2,248 bytes  |  Event Trace: 64/64 slots        |
| Instrument: SOL-USDC             |  Last Update: 11:14:24.123        |
|                                                                       |
| r:reset stats  h:histogram view  s:sparkline view                     |
+-----------------------------------------------------------------------+
```

**Reference**: zenith (histograms), nvtop (sparklines), sampler (real-time monitoring)

### Global Keybindings

| Key | Action |
|-----|--------|
| `1`-`6` | Focus specific panel |
| `Tab` / `Shift+Tab` | Cycle panel focus |
| `Space` | Pause/resume data collection |
| `?` | Toggle help overlay |
| `q` | Quit |
| `z` | Zen mode (fullscreen focused panel) |

### TUI Crate Structure

```
crates/tui/
+-- Cargo.toml
+-- src/
    +-- main.rs              # Entry point + crossterm init
    +-- app.rs               # AppState + panel routing
    +-- input.rs             # Global keybinding handler
    +-- theme.rs             # Color theme (green=ok, red=alert, yellow=warn, blue=info)
    +-- panels/
    |   +-- mod.rs
    |   +-- market.rs        # [1] Price chart + order book
    |   +-- agent_log.rs     # [2] Decision flow + block folding
    |   +-- risk.rs          # [3] Risk potential + Greeks matrix
    |   +-- orders.rs        # [4] Order table + drill-down
    |   +-- research.rs      # [5] Ratchet scatter + hypothesis list
    |   +-- memory.rs        # [6] Shared memory perf monitor
    +-- widgets/
    |   +-- mod.rs
    |   +-- sparkline.rs     # Mini line chart
    |   +-- braille_chart.rs # Braille-character rendered chart
    |   +-- heatmap.rs       # Greeks heatmap
    |   +-- histogram.rs     # Latency distribution histogram
    |   +-- scatter.rs       # Ratchet scatter plot
    |   +-- orderbook.rs     # Order book depth ladder
    |   +-- block_tree.rs    # Warp-style block quote folding tree
    +-- data/
        +-- mod.rs
        +-- mmap_reader.rs   # Shared memory reader (mmap + seqlock)
        +-- jsonl_tail.rs    # JSONL log tail (tokio::fs + notify)
        +-- state_file.rs    # Autoresearch state file reader
```

### TUI Dependencies

```toml
[dependencies]
ratatui = "0.29"
crossterm = "0.28"
color-eyre = "0.6"
tokio = { version = "1", features = ["full"] }
notify = "7"                     # filesystem events for JSONL tail
nautilus-state-encoder = { path = "../state_encoder" }  # mmap reader
```

---

## Development Phases

### P0: Foundation (Now -> June)

**Goal**: Exchange demo working, Nautilus internals understood, basic prototypes.

| # | Task | Deliverable | Verification |
|---|------|-------------|-------------|
| 0.1 | Any exchange demo access (recommended: OKX/Binance Testnet) | `.env` config | Connect WebSocket, receive quotes, place simulated orders |
| 0.2 | Nautilus native strategy | `examples/ma_cross.py` | Simulated fills on demo |
| 0.3 | Understand MessageBus/ExecutionEngine | Code notes | Can trace QuoteTick -> Strategy -> Order -> Fill |
| 0.4 | SwarmStrategy wrapper prototype | `agent_swarm/src/strategy_wrapper.rs` | Registered via `trader.add_strategy()`, receives QuoteTick callbacks |
| 0.5 | StateEncoder msgbus subscriber prototype | `state_encoder/src/subscriber.rs` | `subscribe_quotes()` receives quotes, writes to ContextWindow |
| 0.6 | TUI skeleton | `crates/tui/` | Empty 6-panel layout renders in terminal |

**Supported exchanges**: Binance, OKX, Bybit, dYdX, Interactive Brokers, Betfair, Coinbase, Kraken, Deribit, Polymarket, Hyperliquid — all NautilusTrader-supported exchanges.

### P1: State-as-Context + TUI Basics (July -> August)

**Goal**: Shared memory working, agents reading state, TUI showing live data.

| # | Task | Deliverable | Verification |
|---|------|-------------|-------------|
| 1.1 | SharedStateBuffer cross-process mmap | `state_encoder/src/mmap_shm.rs` | Write -> mmap read, latency <1us |
| 1.2 | Seqlock integration test | `state_encoder/tests/cross_process.rs` | Two processes concurrent read/write, zero torn reads |
| 1.3 | Full quote pipeline | `state_encoder/src/bridge.rs` | WebSocket -> ContextWindow -> mmap file readable |
| 1.4 | Local LLM reads ContextWindow | `agent_swarm/src/perception/llm.rs` | Qwen3-8B reads shared memory, outputs intent |
| 1.5 | Three-layer routing prototype | `agent_swarm/src/perception/router.rs` | 95% Layer 1, 5% Layer 2, <1% Layer 3 |
| 1.6 | TUI Market panel | `tui/src/panels/market.rs` | Live price chart + order book from mmap |
| 1.7 | TUI Agent Log panel | `tui/src/panels/agent_log.rs` | Decision flow with block folding |

**Key metrics**:
- Shared memory write latency: <100ns
- mmap read latency: <50ns
- ContextWindow update rate: >10,000/sec
- TUI frame rate: 60fps sustained

### P2: AgentSwarm Full Loop + TUI Complete (September -> October)

**Goal**: Agents trading autonomously, TUI showing all panels.

| # | Task | Deliverable | Verification |
|---|------|-------------|-------------|
| 2.1 | IntentCompiler two-stage | `agent_swarm/src/compiler.rs` | Template <1us, Almgren-Chriss ~1ms |
| 2.2 | SwarmCoordinator consensus | `agent_swarm/src/swarm.rs` | Same-instrument conflict resolution correct |
| 2.3 | RiskPotentialField integration | `risk_potential/src/feedback.rs` | Gradient -> AgentFeedback, hard_check rejects over-limit |
| 2.4 | End-to-end paper trading | Integration test | Agent -> Intent -> Compiler -> Order -> Fill -> Feedback |
| 2.5 | Decision log JSONL | `sextant_decisions.jsonl` | Every decision recorded with outcome |
| 2.6 | TUI Risk panel | `tui/src/panels/risk.rs` | Real-time potential field + Greeks matrix |
| 2.7 | TUI Orders panel | `tui/src/panels/orders.rs` | Sortable table + drill-down detail |

**Key metrics**:
- Decision latency (perceive -> order): <10ms (Layer 1), <100ms (Layer 2)
- IntentCompiler throughput: >1000 intents/sec
- Zero hard_check violations in 1000 decisions

### P3: Autoresearch + Reputation + TUI Research (January+)

**Goal**: Self-evolving strategies, on-chain reputation, complete TUI.

| # | Task | Deliverable | Verification |
|---|------|-------------|-------------|
| 3.1 | Micro-backtest engine | `autoresearch/src/micro_backtest.rs` | 5-min window, Parquet data replay |
| 3.2 | Ratchet runtime | `autoresearch/src/runtime.rs` | 100 rounds, IR improvement >5% |
| 3.3 | Strategy memory (LCM) | `autoresearch/src/memory.rs` | Successful/failed hypotheses queryable |
| 3.4 | ERC-8004 attestation client | `reputation/src/client.rs` | Testnet attestation submission |
| 3.5 | AutonomyLevel slider | `reputation/src/autonomy.rs` | Score 90+ -> Full autonomy |
| 3.6 | Layer 3 deep LLM integration | `agent_swarm/src/perception/deep_llm.rs` | Qwen3-27B generates executable strategy patches |
| 3.7 | TUI Research panel | `tui/src/panels/research.rs` | Ratchet scatter plot + hypothesis list |
| 3.8 | TUI Memory panel | `tui/src/panels/memory.rs` | Shared memory perf monitor + latency histogram |

---

## Performance Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| ContextWindow write latency | <100ns | `SharedStateBuffer::write()` timing |
| ContextWindow read latency | <50ns | `SharedStateBuffer::read()` timing |
| Seqlock torn reads | 0 | Counter in SharedStateBuffer |
| IntentCompiler Stage 1 | <1us | Template matching timing |
| IntentCompiler Stage 2 | ~1ms | Almgren-Chriss optimization timing |
| RiskPotentialField gradient | ~2ns | Single `#[inline(always)]` call |
| Decision latency (Layer 1) | <10ms | perceive() -> compile() total |
| Decision latency (Layer 2) | <100ms | Including Qwen3-8B inference |
| Decision latency (Layer 3) | <1s | Including Qwen3-27B inference |
| Autoresearch micro-backtest | <5s | 5-min window replay |
| TUI frame rate | 60fps | 16ms refresh cycle |
| Shared memory throughput | >10,000 writes/sec | Sustained under load |

---

## Evaluation Metrics

| Dimension | Metric | Target |
|-----------|--------|--------|
| Re-Discovery | Classic strategy risk-adjusted IR | >= 0.8 |
| New Discovery | IR improvement over baseline | >= 5% per ratchet cycle |
| Cross-Regime Robustness | IR coefficient of variation | < 0.3 |
| Execution Fidelity | Backtest vs paper trade PnL correlation | > 0.85 |
| State Consistency | Shared memory read latency | < 1us |
| Risk Compliance | Hard bottom line triggers per 1000 decisions | 0 |
| Autoresearch Efficiency | Acceptance rate | > 10% (quality over quantity) |

---

## Upstream Sync Strategy

```bash
# Quarterly rebase from upstream
git remote add upstream https://github.com/nautechsystems/nautilus_trader.git
git fetch upstream
git rebase upstream/main

# After resolving conflicts, tag new baseline
git tag sextant-base-v{upstream_version}
git push origin sextant-base-v{upstream_version}

# Update sextant/Cargo.toml tag reference
```

**Conflict hotspots**: `Cargo.toml` (workspace members), `msgbus/api.rs` (if hook added)
**Expected time**: 15-30 minutes per sync

---

## Risk Register

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Exchange Demo API limits | Low | P0 blocker | Multi-exchange fallback |
| Local LLM latency too high | Medium | P1 degraded | Degrade to Layer 1 rule engine |
| LLM decision quality poor | Medium | P1 stop-loss | Keep StateEncoder as standalone data infrastructure |
| Nautilus major refactor | Low | Global | Sextant only depends on core/model/common -- low migration cost |
| TUI rendering performance | Low | UX | Braille rendering + event-driven + 16ms cap |
| Autoresearch overfitting | Medium | Strategy quality | Cross-regime validation, IR variation check |
| On-chain gas costs | Medium | Reputation economics | Batch attestations, L2 deployment |

---

## Stop-Loss Condition

If by end of P1 (August), local LLM decision quality cannot beat a simple moving average strategy:
- Pause Agent development
- Keep StateEncoder as standalone high-performance data infrastructure project
- Reassess approach before P2
