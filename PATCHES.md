# PATCHES.md — local Codex fork customizations

Tracks every custom patch on this fork: rationale, upstream issue, files touched, and the likely
rebase-conflict area when syncing with `openai/codex` upstream.

- **Fork base:** `openai/codex` @ `9e552e9d15ba52bed7077d5357f3e18e330f8f38` (2026-07-11, "Use available width for skill names in the toggle view", #32485).
- **In-tree version:** `0.0.0-dev` (release version is stamped at build time).
- **Branch:** `claude/subagent-routing-v2` (off `main`).
- **Owner:** Claude (engineering owner), for Jessica's engine/game swarm workflow.
- **Toolchain:** rustup-managed `1.95.0` (matches `codex-rs/rust-toolchain.toml`); validate with `just fmt`, `just fix -p <crate>`, `just test -p <crate>`.

## Working rules
- One patch = one coherent, rebase-friendly commit. Behavior changes go behind config/feature flags.
- Keep the hosted-reserved `collaboration.spawn_agent` schema byte-compatible; expanded controls ride a non-reserved namespace / config, never a mutated reserved schema.
- Mechanism (routing correctness) is separate from policy (swarm strategy). Policy is data-driven and reconciled with Jessica's research doc (see Provenance).
- Reproduce each defect against the base commit (failing test proven RED) before changing behavior.

---

## Verified base behavior (read from source @ 9e552e9d1)

| # | Behavior | Location | Status |
|---|---|---|---|
| V1 | `hide_spawn_agent_metadata_options` strips `agent_type`/`model`/`reasoning_effort`/`service_tier` from the model-visible schema | `core/src/tools/handlers/multi_agents_spec.rs:637-641` | verified |
| V2 | Schema hiding gated by `hide_agent_type_model_reasoning`; **default hide = true**, default namespace **`collaboration`** | `core/src/config/mod.rs:251, 1173-1174` | verified |
| V3 | `hide_spawn_agent_metadata` + `tool_namespace` already overridable via `[features.multi_agent_v2]` | `features/src/feature_configs.rs:62-64`; resolved `core/src/config/mod.rs:2531-2537` | verified |
| V4 | `fork_mode()` maps omitted/blank `fork_turns` → `"all"` → `Some(FullHistory)` | `core/src/tools/handlers/multi_agents_v2/spawn.rs:199-211` | verified |
| V5 | `FullHistory` branch calls `reject_full_fork_spawn_overrides(role, model, effort)` → errors if ANY of the three is set | `spawn.rs:67-72`; helper `multi_agents_common.rs:193-204` | verified |
| V6 | Non-full-history path applies model/effort overrides (validating availability + supported effort) then applies the role | `spawn.rs:73-85`; `multi_agents_common.rs:234-267` | verified |
| V7 | `agent_type` selects the role; `task_name` is canonical path / display only | `spawn.rs:53-57, 100` | verified |
| V8 | Unknown `agent_type` errors before the child is spawned (fresh/partial path) | `agent/role.rs:44-46`, reached at `spawn.rs:82` | verified |

**Root-cause summary.** Two compounding issues, only one of which is a client-side logic bug:
1. **Omitted-fork sharp edge (client-fixable logic bug):** omitting `fork_turns` while requesting a role/model/effort is rejected outright, because the omission silently becomes a full-history fork (V4+V5). This is the primary Phase-1 repair. (Upstream #32031, #20077.)
2. **Hidden routing schema (mostly config + a hosted-backend boundary):** by default the model-visible schema hides the routing fields (V1+V2). This is already toggleable via config (V3); the part that CANNOT be fixed in this repo is that the hosted GPT-5.6 path treats `collaboration.spawn_agent` as reserved and rejects a modified schema — so exposing fields requires a non-reserved namespace. (Upstream #31814, #31893.)

Corrections to the incoming handoff framing (both verified above): role resolution is **not** broken (V7), and unknown roles **already** fail closed on the fresh path (V8) — so the fix must not touch the handler's role logic.

---

## Repro → source mapping (routing-matrix.csv), status on base

Derived from source (pending execution once the crate is built):
- **Reproduces the bug (base rejects instead of spawning fresh):** R02, R05, R06 — omitted `fork_turns` + explicit `agent_type`/`model`/`effort`.
- **Already correct on base:** R01, R03, R04, R07, R08, R10, R16 (R12/R13 pending role fixtures).
- **Not a base bug — needs new Phase-2 strict/budget features:** R09 (`require_explicit_agent_type`), R14/R15 (unavailable model / unsupported effort — note V6 already rejects these on the fresh path).
- **Not reproducible in this environment (needs live ChatGPT auth + GPT-5.6):** R11 (hosted reserved-schema rejection).

---

## Patch log

> Status legend: `planned` → `red` (failing test in place) → `landed` (committed, tests green).

### C0+C1 — intent-aware fork default + tests — _validated, ready to commit (single green commit)_ — upstream #32031
C0 (failing tests) and C1 (fix) land as **one** commit because contributing.md requires each commit to
compile and pass tests (a tests-only commit would leave the suite red).

**Behavior change:** in `SpawnAgentArgs::fork_mode`, an omitted/blank `fork_turns` is now intent-aware —
if `agent_type`/`model`/`reasoning_effort` is explicitly set it resolves to a **fresh** child
(`fork_mode = None`) so the override is honored; with no routing override it keeps the inherited
full-history default. Explicit `fork_turns` is unchanged: `none` => fresh, `all` => full history (still
rejects overrides via `reject_full_fork_spawn_overrides`), positive int => partial.

- Files:
  - `core/src/tools/handlers/multi_agents_v2/spawn.rs` — `fork_mode` intent-aware default + doc.
  - `core/src/tools/handlers/multi_agents_v2/spawn_tests.rs` (new) — 10 unit tests over the fork-resolution matrix rows.
  - `core/src/tools/handlers/multi_agents_tests.rs` — repurposed `multi_agent_v2_spawn_defaults_to_full_fork_and_rejects_child_model_overrides` (asserted the old sharp edge) into `multi_agent_v2_spawn_omitted_fork_with_route_creates_fresh_child` (asserts the fix at the handler level).
- Rebase risk: **medium** — `spawn.rs` and `multi_agents_tests.rs` are active upstream; change is localized to `fork_mode` + one test.

**Validation (base `9e552e9d1`, toolchain 1.95.0):**
- Repro (red): the 4 new omitted-fork cases failed on base (`Some(FullHistory)` where `None` required).
- After fix (green): all 10 `spawn_tests` pass; repurposed handler test passes; `multi_agent_v2_spawn_fork_turns_all_rejects_agent_type_override` still passes.
- Full `codex-core` lib suite via nextest: **2017 passed, 2 failed**. The 2 failures are `shell_snapshot::tests::{try_create_creates_and_deletes_snapshot_file, try_create_uses_distinct_generation_paths}` — environmental (`"validation_failed"` spawning a real shell under the sandbox), untouched by this diff.
- `cargo fmt -p codex-core`: clean. `cargo clippy -p codex-core --tests`: no warnings in changed files.

### C2 — resolved-route record + provenance — _validated, ready to commit_ — deliverable 5
Adds `ResolvedSubagentRoute { task_name, agent_type, model, reasoning_effort, service_tier, fork_mode, agent_config_path, warnings }`, each routed value tagged with a `RouteSource` provenance
(`explicit_spawn_argument` / `custom_agent_file` / `parent_inheritance` / `model_catalog_default` /
`client_default`). Effective values are read from the child's runtime `ThreadConfigSnapshot` (source of
truth — not the child's self-report); provenance is derived by comparing them against the explicit
request, the parent baseline, and whether a role file was applied. A mismatch between an explicit
request and the effective value is recorded as a `warnings` entry (the research doc's #1 silent-
substitution failure, surfaced as data). Returned only on the **un-hidden** `WithNickname` result, so
the reserved `collaboration` (`HiddenMetadata`) schema is byte-for-byte unchanged.

- Files:
  - `core/src/tools/handlers/multi_agents_v2/resolved_route.rs` (new) — record + `RouteSource` + pure `resolve()`.
  - `core/src/tools/handlers/multi_agents_v2/resolved_route_tests.rs` (new) — 7 provenance/serialization tests.
  - `core/src/tools/handlers/multi_agents_v2.rs` — `mod resolved_route;`.
  - `core/src/tools/handlers/multi_agents_v2/spawn.rs` — capture requested route + parent baseline before spawn; build the route from the snapshot; add `route: Option<Box<ResolvedSubagentRoute>>` to `WithNickname` (boxed to satisfy `large_enum_variant`).
  - `core/src/tools/handlers/multi_agents_tests.rs` — integration test asserting the un-hidden result reports the route with provenance.
- **Deviation from contract:** `permission_profile` and per-field usage/cost are **deferred to C4** (surfacing). `PermissionProfile` is a complex generic enum that needs display formatting, and it carries little routing signal (always the parent's live runtime profile). Documented so the deferral is explicit.
- Rebase risk: **medium** — `spawn.rs` result shape + handler are active upstream; the record is a new isolated module.

**Validation:** 7 record tests + 1 integration test pass; provenance derivation and camelCase/snake_case serialization verified. Full `codex-core` lib via nextest: **2025 passed, 2 failed** (the same environmental `shell_snapshot` tests). `fmt` clean; `clippy -p codex-core --tests` clean on changed files.

### C3 — persist resolved route (versioned) — _planned_ — deliverable 5/6
Persist the resolved route in rollout/session metadata so resumed sessions/UIs don't depend on ephemeral tool output.
- Files: `protocol/`, `rollout/`, `state/`, `core/src/agent/`.
- Rebase risk: **high** — protocol/rollout are churn hotspots; version the persisted shape.

### C4 — surface resolved route (TUI) — _deferred_ — deliverable 6
Investigation finding: the rich "Spawned" card (role · model · effort) is emitted only by the **V1**
spawn path (`multi_agents/spawn.rs` → `CollabAgentSpawnBegin`/`CollabAgentToolCall`). A **V2** spawn
(the configured surface) emits only `SubAgentActivity` → the sparse "Started `{agent_path}`" cell,
which has no route fields. Every sub-agent datum the TUI shows crosses the app-server `ThreadItem`
boundary (TS-exported), so surfacing the route for V2 requires threading it through
`SubAgentActivityItem` (protocol + app-server schema regen + TUI + snapshots). Deferred in favor of C5
(additive, higher value); the orchestrator already sees the resolved route + warnings via C2's
un-hidden tool output.
- Files (when done): `protocol/src/items.rs`, `app-server-protocol/`, `core/.../spawn.rs`, `tui/src/multi_agents.rs`, snapshots.
- Rebase risk: high (protocol + TUI hotspots).

### C5a — strict routing: require explicit agent_type — _validated, ready to commit_ — deliverable 8
First strict-mode slice, realizing the research doc's "no child model by inheritance in strict mode."
Adds `features.multi_agent_v2.require_explicit_agent_type` (default **false**, opt-in). When enabled, a
`spawn_agent` with no `agent_type` is rejected **before** a child thread is created — forcing an
explicit role instead of a generic inherited child. Default behavior unchanged.
- Files:
  - `features/src/feature_configs.rs` — `require_explicit_agent_type: Option<bool>` on `MultiAgentV2ConfigToml`.
  - `core/src/config/mod.rs` — field on `MultiAgentV2Config` (default false) + resolver.
  - `core/src/tools/handlers/multi_agents_v2/spawn.rs` — pre-spawn gate.
  - `core/src/tools/handlers/multi_agents_tests.rs` — rejection test.
  - `core/config.schema.json` — regenerated (`just write-config-schema`).
- Rebase risk: **medium** — `config/mod.rs` is very high churn; reuses the existing `multi_agent_v2` config plumbing.

### C5b — remaining strict checks + budget breakers — _planned, provisional_ — deliverable 8
Reject explicit model/effort silent-substitution pre-spawn (role-pin conflict); `[agents.budget]` caps
(per-root spawn cap, per-model/write concurrency, depth, allowlist); optional route preflight/dry-run.
**Names/defaults reconcile with `agent-swarm-orchestration-lessons.md`** (per-agent honored-or-fail-loudly,
declared availability fallback, no expensive child by inheritance).
- Files: `config/`, `features/`, `core/src/agent/`.
- Rebase risk: medium (additive config + new module).

---

## Rebase-conflict watch (upstream hotspots this stack touches)
- `core/src/tools/handlers/multi_agents_v2/spawn.rs` — C1/C2 (actively developed upstream).
- `core/src/config/mod.rs` — C5 (very high churn).
- `protocol/src/protocol.rs`, `rollout/`, `state/` — C3 (high churn; version persisted metadata).
- `tui/src/chatwidget.rs`, `app-server/src/codex_message_processor.rs` — C4 (hotspots; add modules).

## Hosted-backend boundary (cannot be fixed in this repo)
The hosted GPT-5.6 path treats `collaboration.spawn_agent` as reserved and (per the reported error)
rejects a modified schema under ChatGPT auth. The client keeps the reserved schema byte-compatible,
exposes expanded fields only on a non-reserved namespace (e.g. `agents`), and **fails closed with a
clear warning** rather than degrading. Whether the `agents` namespace is accepted across Jessica's
surfaces (CLI/TUI/Desktop/IDE/ChatGPT-auth/API-key) is reported-but-unverified and requires live
smoke tests (see docs/06 acceptance tests).

## External prior art — NOT vendored
Issue #32031 links `lidge-jun/codex@fde7de4d0`. Treated as reference only; reimplemented against this
base. No external branch is fetched or applied.

## Research provenance (Phase-2 policy input)
- File: `codex-subagent-handoff-2026-07-12/agent-swarm-orchestration-lessons.md`
- Date/version: 2026-07-12 (mtime 10:22), 12554 bytes
- SHA-256: `74c3fc5bbcca1910dae01c0492ddddb9c1136f5115c207a77e2c310b91f2d50b`
- Supersession: refines `docs/08_SWARM_PRODUCT_REQUIREMENTS.md` (baseline). Its #1 requirement
  ("per-agent model AND effort honored exactly, or the spawn fails loudly — never silently
  substitute") is the design north-star for C1/C5. Full traceability table to be produced before C5.
