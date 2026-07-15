# PATCHES.md — local Codex fork customizations

Tracks every custom patch on this fork: rationale, upstream issue, files touched, and the likely
rebase-conflict area when syncing with `openai/codex` upstream.

- **Current fork base:** `bbfa08fe3` ("Include connector IDs in MCP tool call analytics", #32867). The consolidated customization `70136efb` was replayed as `be86cc422`; the proposal commit `a4ade218f` was replayed as `6da71ab5b`.
- **Historical development base:** `c888e8e75` ("Improve composer completion target resolution", #32628). The verified-base evidence below retains its original commit references so the audit trail remains reproducible.
- **In-tree version:** `0.0.0-dev` (release version is stamped at build time).
- **Synchronization branch:** `codex/sync-openai-main-2026-07-13`.
- **Owner:** Claude (engineering owner), for Jessica's engine/game swarm workflow.
- **Toolchain:** rustup-managed `1.95.0` (matches `codex-rs/rust-toolchain.toml`); validate with `just fmt`, `just fix -p <crate>`, `just test -p <crate>`.

## Working rules
- One patch = one coherent, rebase-friendly commit. Behavior changes go behind config/feature flags.
- Keep the hosted-reserved `collaboration.spawn_agent` schema byte-compatible; expanded controls ride a non-reserved namespace / config, never a mutated reserved schema.
- Mechanism (routing correctness) is separate from policy (swarm strategy). Policy is data-driven and reconciled with Jessica's research doc (see Provenance).
- Reproduce each defect against the base commit (failing test proven RED) before changing behavior.

### Synchronization-only size exception

The replayed history exceeds the normal 800-line review guideline because this branch preserves two
already-reviewed fork commits as directed: the consolidated implementation (`70136efb`, replayed as
`be86cc422`) and its orchestration proposal (`a4ade218f`, replayed as `6da71ab5b`). Splitting either
replay would destroy the existing review and supersession boundary. The new synchronization work is
kept as a separate integration commit: its focused conflict corrections are under the 500-line
complex-logic target. Subsequent audited gaps remain separate milestone branches rather than being
added to this synchronization replay.

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

**Near-term context-handoff policy:** heterogeneous full-history forks are intentionally deferred.
When a parent selects a different child role, model, or effort, it should use `fork_turns = "none"`
and write a concise, task-specific context handoff in `message`: the objective, relevant paths or
symbols, applicable constraints, established findings, and expected output. This preserves the cost
and quality benefits of specialist routing without inheriting stale parent system/developer context
or undertaking the larger prompt-cache and tool-surface work needed for safe routed full-history
forks. Supporting `fork_turns = "all"` with a different route remains a future feature, not part of
the current remediation milestones.

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

### C5b — strict routing: reject route substitution — _validated, ready to commit_ — deliverable 8
Adds `features.multi_agent_v2.reject_route_substitution` (default **false**, opt-in). When enabled, a spawn
whose explicit `model`/`reasoning_effort` conflicts with the selected role's **pinned** value is rejected
**before** a child is created, instead of silently substituting the role's value (research doc #1: "never
silently substitute"). Detection reads the role's *declared* pins (deterministic — no dependency on the
config-layer rebuild internals), so it never false-rejects a role that doesn't pin the field.
- Files:
  - `features/src/feature_configs.rs` (field) + `features/src/tests.rs` (initializer — also backfills the
    C5a `require_explicit_agent_type` field the codex-core-only test run had missed).
  - `core/src/config/mod.rs` (field + resolver); `core/config.schema.json` (regenerated).
  - `core/src/agent/role.rs` — `role_pinned_model_and_effort` helper.
  - `core/src/tools/handlers/multi_agents_v2/spawn.rs` — pre-spawn gate.
  - `core/src/tools/handlers/multi_agents_tests.rs` — conflict-rejection test.
- Rebase risk: medium.
- **Process note:** touching `features/` requires running the `codex-features` tests, not just `codex-core`.

### C5c — shallow spawn policy — _validated, ready to commit_ — deliverable 8
Removes the unintended per-root lifetime spawn quota: completed children never consume a permanent
budget. The retired config key remains accepted but ignored for compatibility; lifetime usage is
unbounded and the existing active-thread concurrency limiter remains authoritative.
Multi-agent V2 uses a dedicated `features.multi_agent_v2.max_depth` setting (default **2**, minimum
**1**, maximum **4**). At the default, the root can spawn a depth-one orchestrator and that
orchestrator can spawn depth-two workers; maximum-depth worker schemas omit `spawn_agent`.
Replayed/direct spawns beyond the configured depth fail before creating a thread. The legacy
`[agents].max_depth` setting does not control this V2 boundary. Model routing remains independent:
every spawn may explicitly select a different role/model/effort, so a Sol root or orchestrator can
route reader work directly to Terra without inheriting its own model.

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

---

## Append-only record

_This section is append-only: entries are added, never edited or removed. Established per Jess's
2026-07-14 direction after the 2026-07-13 synchronization rewrote header material in place._

### Correction — provenance of prior decisions (2026-07-14)

Recorded per Jess's rulings, 2026-07-14:

- **User-directed (stand as designed):** the MultiAgent V2 depth cap
  (`features.multi_agent_v2.max_depth`, default **2**) and unlimited sequential spawn identities
  were both directed by Jess — not agent-improvised.
- **Agent decisions (now adjudicated):**
  - The `max_total_spawns_per_root` compatibility field ("accepted but ignored", C5c) was an agent
    decision made without a stamp. Ruling: a dead knob may not remain accepted-but-ignored — it must
    be removed or restored as real enforcement. Jess recommends removal; Fable concurs (2026-07-14).
    Removal lands as its own PR referencing this entry. Note: `MultiAgentV2ConfigToml` is
    `deny_unknown_fields`, so removal is deliberately loud — configs still setting the key fail to
    parse with an error naming it.
  - The 2026-07-13 in-place rewrite of this file's header block (fork-base restructure, the
    synchronization-size-exception section, the near-term context-handoff policy) was an agent
    decision. The content stands as written, but the in-place edit set no precedent: from this entry
    forward, historical corrections to PATCHES.md are append-only.
- **Confirmed semantics (C2):** the resolved-route record warns on explicit-request/effective
  mismatches while reporting the actual winning provenance. Confirmed by Jess, 2026-07-14.

### Sync record — 2026-07-14 (openai/codex upstream)

Supersedes the header's "Current fork base" line (retained above unedited, per append-only rule).

- **New fork base:** `8604689ec5` ("Allow injecting the Codex Apps tools cache", #33113) —
  34 upstream commits absorbed since `60b9b551c1` (#32875).
- **Replayed fork commits:** `9fe9f58b27` (#1 bounded nesting), `6ef53835c7` (#2 proposal),
  `514f8bb0f6` (overlap resolution), `38fa5204ac` (#3 shallow-policy cleanup). Zero manual
  conflicts.
- **Validation:** 48-agent commit review (33 dossiers, 14 adversarial deep-reads, synthesis);
  focused fork surface 333/333; full workspace suite 12,137/12,139 — the 2 failures are
  `codex-otel` OTLP-loopback tests proven sync-independent (crate and dependency closure untouched
  by the sync); 7/7 fork invariants re-verified at file:line on the merged tree.
- **Published:** force-with-lease to `origin/main` (`f1f45fe01e` → `38fa5204ac`), 2026-07-14.
  Rollback ref: `refs/backup/pre-sync-8604689-2026-07-14`.
- **Watch items from review:** #33076 `AgentRunner::start` spawns below the fork's gates (inert
  in-tree today; gate-relocation into `ThreadManager::spawn_subagent` proposed, pending Jess's
  stamp); upstream SQLite thread-history (#32896/#32923/#32928) overlaps planned C3 — reconcile
  before C3 work begins; #33109 exposes a pre-existing unguarded paginated-parent fork path
  (tracked separately).

### Removal — `max_total_spawns_per_root` (2026-07-14)

Executes the correction ruling above: the retired lifetime-quota key is removed rather than
restored (Jess's recommendation; Fable concurs). `MultiAgentV2ConfigToml` is `deny_unknown_fields`,
so configs still setting the key now fail to parse, with the error pointing at the
`[features.multi_agent_v2]` table (serde's untagged-enum error does not name individual fields) —
deliberate loud breakage, covered by `multi_agent_v2_rejects_retired_lifetime_quota_key`. The active-thread
concurrency limiter and `features.multi_agent_v2.max_depth` remain the authoritative bounds.
