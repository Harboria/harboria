[English](README.md) | [简体中文](README_zh-CN.md)
---

# Harboria

**面向 MicroDuck 的长期个人 AI 运行时。**

Harboria 是一个开源运行时，用于构建能够长期维护上下文、跟踪用户状态变化，并对何时以及如何进行介入做出结构化决策的个人 AI 系统。

当前实现面向 **[MicroDuck](https://github.com/pollen-robotics/microduck)** 开发。MicroDuck 是 **Pollen Robotics** 开源的小型双足机器人。

## 概述

大多数 AI Agent 都围绕短周期交互设计：

```text
用户
 ↓
请求
 ↓
Agent
 ↓
任务
 ↓
结果
```

Harboria 面向的是更长期的运行过程：

```text
用户
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

当前版本的 Harboria 与 **MicroDuck** 集成。

MicroDuck 负责机器人侧的运行时以及面向硬件的能力，Harboria 位于其上层，负责长期个人 AI 相关的运行时能力。

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

## 运行

```bash
cargo build

# 可选：不设置就自动退回规则/占位实现，不会报错
export ANTHROPIC_API_KEY=sk-ant-...
export HARBORIA_ANTHROPIC_MODEL=claude-sonnet-5   # 按需覆盖
export HARBORIA_TICK_INTERVAL_SECS=300            # 自动评估的间隔

# 终端 1
cargo run --bin harboriad

# 终端 2：一个完整的场景
GOAL=$(cargo run --bin harboria-cli -- goal-create "每天冥想" --goal-type habit \
  | python3 -c "import json,sys;print(json.load(sys.stdin)['result']['id'])")
cargo run --bin harboria-cli -- state-add-routine 8 "通常8点冥想"
cargo run --bin harboria-cli -- tick                       # 命中 ScheduledWindowMatch
cargo run --bin harboria-cli -- intervention-respond "$GOAL" accepted
cargo run --bin harboria-cli -- task-log "$GOAL" "第一次" postponed
cargo run --bin harboria-cli -- task-log "$GOAL" "第二次" postponed
cargo run --bin harboria-cli -- task-log "$GOAL" "第三次" postponed
cargo run --bin harboria-cli -- tick                       # 命中 RepeatedAvoidance
cargo run --bin harboria-cli -- progress-add "$GOAL" milestone 1

# 不手动 tick，靠自动触发
cargo run --bin harboria-cli -- state-set-availability true

# 闲聊，不影响任何 Goal
cargo run --bin harboria-cli -- chat "你好"
```

## 许可证

Apache-2.0
