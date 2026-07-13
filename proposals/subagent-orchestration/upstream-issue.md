# Upstream subagent-orchestration issue package

> Internal draft. Do not post automatically. OpenAI accepts external code contributions by
> invitation only, so the recommended first action is a focused issue comment with evidence and a
> narrowly staged offer to help.
>
> Last duplicate check: 2026-07-13 against open and closed `openai/codex` issues.

## Recommended upstream path

1. Post the focused implementation offer below on
   [#32027, `agents.max_depth = 1` permits child-to-grandchild spawning](https://github.com/openai/codex/issues/32027).
   This is the smallest independently landable gap demonstrated by the fork.
2. Open the standalone umbrella issue only if maintainers want a shared integration tracker. It is
   intentionally framed as a map of existing issues, not as one omnibus implementation request.
3. Treat [#32100, Orchestrated multi-agent mode PoC](https://github.com/openai/codex/issues/32100)
   as adjacent prior art, not the posting target. Its sequential internal phase machine explicitly
   avoids user-facing subagent spawning/polling, so a configurable nested tree would be off-topic.

This route avoids reopening requests that already have dedicated tracking:

| Concern | Existing tracking | Current interpretation |
|---|---|---|
| Configurable recursion depth | [#9912](https://github.com/openai/codex/issues/9912), closed as completed | The setting exists, but V2 enforcement is still reported broken. |
| V2 and alternate-path depth bypass | [#32027](https://github.com/openai/codex/issues/32027), [#30692](https://github.com/openai/codex/issues/30692) | Exact first implementation slice. Derived/forked autonomous workers may need a follow-up path. |
| Independent child model and effort | [#31814](https://github.com/openai/codex/issues/31814), [#32782](https://github.com/openai/codex/issues/32782) | [#32749](https://github.com/openai/codex/pull/32749) and [#32751](https://github.com/openai/codex/pull/32751) are merged to upstream `main` and expose compatible model/effort overrides; named `agent_type` routing remains separately reported. |
| Capacity semantics and preflight | [#22779](https://github.com/openai/codex/issues/22779), [#23479](https://github.com/openai/codex/issues/23479), [#32353](https://github.com/openai/codex/issues/32353) | Active execution, retained identity, residency, and lifetime-created counts should not be conflated. |
| Cold resume and descendant coordination | [#24281](https://github.com/openai/codex/issues/24281), [#26718](https://github.com/openai/codex/issues/26718), [#32203](https://github.com/openai/codex/issues/32203) | [#32837](https://github.com/openai/codex/pull/32837) restores V2 descendant identities on cold root resume, but route restoration and requester-aware completion remain separate concerns. |
| Canonical route/status visibility | [#32504](https://github.com/openai/codex/issues/32504), [#32488](https://github.com/openai/codex/issues/32488), [#29540](https://github.com/openai/codex/issues/29540) | The canonical activity and UI surfaces still need the effective route and nested lifecycle state. |

## Ready-to-post focused comment for #32027

I reproduced this as a MultiAgent V2 admission/tool-exposure mismatch and have a reference fix in a
fork. Would maintainers be open to an invited, depth-only PR?

The merged reference is
[jessatech/codex-dev#1](https://github.com/jessatech/codex-dev/pull/1) at
[`70136efb`](https://github.com/jessatech/codex-dev/commit/70136efbb6672c41ca987f9d1365a95d37405e8f).
The depth-specific development commits are
[`1d47fae0`](https://github.com/jessatech/codex-dev/commit/1d47fae00052eb42b2ee6a066f1acaca6a32ad84)
and
[`0d76468b`](https://github.com/jessatech/codex-dev/commit/0d76468b918deff4785a5a7c40086143d0c98f38).

The implementation demonstrates:

- rejecting V2 child creation beyond the configured depth before a thread is created;
- omitting `spawn_agent` from model-visible tools at maximum depth while keeping management tools;
- applying the same depth gate to CSV job fan-out;
- capturing the root's limit in the shared agent control, so a role config cannot override it; and
- root -> child -> grandchild, leaf-schema, and role-layer regression coverage.

I would not submit the fork's 26-file squash. A proposed upstream PR would port only the depth
admission design, rebase it onto current `main`, and preserve the exact configuration contract
reported here: `[agents].max_depth = 1` must allow root -> child and reject child -> grandchild.
That makes `Fixes #32027` verifiable without introducing a second depth key or changing the current
default in the bug fix.

The fork separately prefers a configurable 1-4 V2 range with default 2 for root -> orchestrator ->
leaf workflows. I would leave that configuration/default change to a follow-up enhancement unless
maintainers explicitly want it in the invited fix.

That follow-up is consistent with the newly published
[Responses API Multi-agent guide](https://developers.openai.com/api/docs/guides/responses-multi-agent):
it models hierarchical paths including a grandchild, distinguishes an active-concurrency limit
across the entire tree from total-created and depth limits, and documents the same spawn, message,
follow-up, wait, interrupt, and list collaboration primitives. The API guide does not establish
Codex's configuration or heterogeneous model-routing semantics; it is evidence that nested agent
trees and concurrency-as-a-separate-bound are intentional product concepts.

This depth-only PR would reference, not claim to close,
[#30692](https://github.com/openai/codex/issues/30692), because derived/forked autonomous worker
creation may require a separate shared-admission change.

## Standalone umbrella issue draft

Use this only if maintainers want a separate integration tracker rather than independent discussion
on the existing issues.

### Proposed title

MultiAgent V2 orchestration policy is split across depth, routing, capacity, and resume paths

### Proposed body

#### Problem

MultiAgent V2 has useful delegation primitives, but a bounded nested workflow cannot yet be
described or audited as one runtime contract. Configuration, tool exposure, child creation,
role/model routing, capacity, persistence, completion delivery, and status each make partly
independent decisions.

This makes apparently safe configurations unreliable. A child may retain `spawn_agent` beyond the
intended depth; completed, resident, and lifetime-created agents can be mistaken for the same
budget; route intent is not visible on every V2 surface; and a resumed root may recover a descendant
identity without recovering the full effective route or requester/completion relationship.

#### Use case

The target workflow is a deliberately bounded tree:

```text
root planner
  -> bounded orchestrator
       -> bounded reader, implementer, or verifier
```

Desired properties:

- configurable shallow depth, with 1-4 and default 2 proposed for discussion;
- a root-owned cap that role layers cannot change;
- active concurrency separate from lifetime-created, completed, and resident agent counts;
- no implicit permanent lifetime spawn cap;
- independent role/model/effort selection at every allowed edge; and
- durable, inspectable identities, routes, admission state, and completion delivery.

This is not a request for unlimited recursive swarms. The goal is predictable delegation whose
tree shape, cost, and authority remain bounded.

#### Product-direction evidence

OpenAI's beta
[Responses API Multi-agent guide](https://developers.openai.com/api/docs/guides/responses-multi-agent)
documents a root that can create and coordinate a tree of parallel subagents. Its example hierarchy
includes `/root/reviewer/tester`, and its `max_concurrent_subagents` setting limits active descendants
across the entire tree while excluding the root. The guide explicitly treats tree depth, total agents
created, and active concurrency as different concepts, and exposes the same six collaboration
primitives used by MultiAgent V2.

That does not prove that Codex should copy the Responses API defaults, model sharing, or tool sharing.
It does show that bounded nested orchestration is aligned with a current OpenAI product direction,
while the Codex-specific issue is making local policy enforceable and observable.

#### Existing tracking

The component failures are already tracked in #32027, #30692, #32782, #22779, #23479, #24281,
#26718, #32203, #32353, #32504, and #32488. PR #32837 appears to address cold-resume identity and
lazy reload specifically. Issue #32100 explores a different, sequential internal orchestration
architecture. This issue would only serve as an integration contract and staging map; each
component should remain independently landable and close only its own issue.

#### Reference implementation

A fork implementation is available in
[jessatech/codex-dev#1](https://github.com/jessatech/codex-dev/pull/1). It demonstrates direct V2
depth admission, root-owned policy, a bounded 1-4 V2 configuration, model-independent nested
handler routing when the explicit fields are available, and removal of the fork's custom permanent
lifetime spawn quota. It does not prove those routing fields are exposed on every hosted surface and
is not proposed as one upstream PR; it needs rebasing and splitting.

#### Suggested staging

1. Fix #32027 by making V2 honor the existing depth setting in shared admission and tool exposure.
2. Reconcile alternate autonomous child-creation paths from #30692.
3. Finish named role/route visibility and persist the effective route.
4. Add structured capacity/admission preflight with separate active, resident, completed, and
   lifetime-created counters.
5. Complete nested requester/result delivery and canonical UI/telemetry surfacing.
6. Discuss a V2-specific 1-4 depth range and default 2 separately from the compatibility fix.

Would a shared integration tracker be useful, or should this remain a cross-linked set of focused
issues? If the first depth slice is aligned and high priority, I am available to prepare a focused
PR by invitation.
