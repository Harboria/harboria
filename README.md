[English](README.md) | [简体中文](README_zh-CN.md)
---

# Harboria

**Long-term personal AI runtime for MicroDuck.**

Harboria is an open-source runtime for building personal AI systems that maintain long-term context, track changing user state, and make structured decisions about when and how to intervene.

The current implementation is built for **[MicroDuck](https://github.com/pollen-robotics/microduck)**, the open-source biped robot from **Pollen Robotics**.

## Overview

Most AI agents are designed around short-lived interactions:

```text
User
  ↓
Request
  ↓
Agent
  ↓
Task
  ↓
Result
```

Harboria models a longer-running interaction:

```text
User
  ↓
Intent
  ↓
Context
  ↓
User State
  ↓
Opportunity
  ↓
Decision
  ↓
Intervention
  ↓
Feedback
```

## MicroDuck

Harboria currently integrates with **MicroDuck**.

MicroDuck provides the robot-side runtime and hardware-facing capabilities. Harboria provides the long-term personal AI runtime layer above it.

```text
┌─────────────────────────────┐
│          Harboria           │
│                             │
│ Intent                      │
│ Goals                       │
│ Context                     │
│ User State                  │
│ Opportunity                 │
│ Decision                    │
│ Intervention                │
│ Feedback                    │
└──────────────┬──────────────┘
               │
               ▼
┌─────────────────────────────┐
│          MicroDuck          │
│                             │
│ Robot Runtime               │
│ Hardware                    │
│ Devices                     │
│ Motion                      │
│ Communication               │
└─────────────────────────────┘
```

## Development

Harboria is written in Rust.

## Workspace Structure

```text
crates/
  harboria-core          Goal/UserState/Opportunity/Decision domain models +
                           all traits known to Core + pure-rule default implementations + main pipeline
  harboria-embodiment     Embodiment trait, zero hardware dependencies
  harboria-execution      Tool/ExecutionEngine trait, zero concrete execution library dependencies
  harboria-inference      Concrete implementations of ReasoningProvider
  harboria-storage        SQLite implementation, local-first
  harboria-protocol       JSON-RPC wire types for harboriad <-> harboria-cli

adapters/
  microduck               Embodiment implementation

bins/
  harboriad               Long-running process, the actual product
  harboria-cli             Debug console, not a product UI
```

Core rule of dependency direction: traits are defined by the party that "owns the business concept", and the party that implements it depends on that crate in turn. `harboria-core` only depends downward on two self-contained, zero-back-reference trait crates (`harboria-embodiment` / `harboria-execution`); `harboria-storage` / `harboria-inference` / `adapters/microduck` all depend in turn on `harboria-core` or `harboria-embodiment`, not the other way around.

## Running

```bash
cargo build

# Optional: if not set, automatically falls back to rule/placeholder implementations, no error
export ANTHROPIC_API_KEY=sk-ant-...
export HARBORIA_ANTHROPIC_MODEL=claude-sonnet-5   # override as needed
export HARBORIA_TICK_INTERVAL_SECS=300            # interval for automatic evaluation

# Terminal 1
cargo run --bin harboriad

# Terminal 2: a complete scenario
GOAL=$(cargo run --bin harboria-cli -- goal-create "meditate daily" --goal-type habit \
  | python3 -c "import json,sys;print(json.load(sys.stdin)['result']['id'])")
cargo run --bin harboria-cli -- state-add-routine 8 "usually meditate at 8"
cargo run --bin harboria-cli -- tick                       # hits ScheduledWindowMatch
cargo run --bin harboria-cli -- intervention-respond "$GOAL" accepted
cargo run --bin harboria-cli -- task-log "$GOAL" "first time" postponed
cargo run --bin harboria-cli -- task-log "$GOAL" "second time" postponed
cargo run --bin harboria-cli -- task-log "$GOAL" "third time" postponed
cargo run --bin harboria-cli -- tick                       # hits RepeatedAvoidance
cargo run --bin harboria-cli -- progress-add "$GOAL" milestone 1

# without manual tick, relies on automatic triggering
cargo run --bin harboria-cli -- state-set-availability true

# casual chat, does not affect any Goal
cargo run --bin harboria-cli -- chat "hello"
```

## License

Apache-2.0
