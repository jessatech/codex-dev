# Invitation-ready upstream PR draft

> Internal draft. Do not open this PR against `openai/codex` without an explicit maintainer
> invitation. Rebase and rebuild the depth slice from current upstream `main`; do not submit the
> fork's 26-file squash.

## Proposed title

Enforce `agents.max_depth` for MultiAgent V2 spawns

## Intended issue links

```text
Fixes #32027
Refs #30692
Refs #32100
```

Keep #30692 as `Refs` unless all derive/fork autonomous-worker paths are routed through the same
admission check.

## Draft PR body

### What?

Make MultiAgent V2 honor the existing root session `agents.max_depth` value consistently at
model-visible tool planning and runtime spawn admission.

- Allow the root to spawn direct children up to the configured maximum depth.
- Allow a thread-spawn child to orchestrate only while its depth is below that maximum.
- Omit `spawn_agent` from maximum-depth agents while retaining agent management tools.
- Reject an over-depth direct spawn and in-scope CSV fan-out before creating a child thread.
- Prevent selected role configuration from widening or narrowing the root session's topology policy.

This PR does not introduce a new configuration key, change the existing default, or change active
concurrency, residency, cleanup, or lifetime accounting.

### Why?

`[agents].max_depth = 1` is expected to allow root -> child while rejecting child -> grandchild, but
MultiAgent V2 permits the grandchild path reported in #32027. A model-visible `spawn_agent` tool at
a forbidden depth also invites a call that the runtime must reject.

The
[Responses API Multi-agent guide](https://developers.openai.com/api/docs/guides/responses-multi-agent)
now documents hierarchical agent paths including a grandchild and treats active concurrency across
the tree separately from total-created and depth limits. This PR does not copy the API's unlimited
depth behavior; it makes Codex honor the user's existing local depth policy while preserving nested
orchestration when that policy allows it.

Depth is a root orchestration policy, not a role preference. If a child role can override the value
through its config layer, the same root tree can silently become either less capable or less bounded
than the user selected.

### How?

1. Introduce or reuse one exhaustive predicate for whether the current session source may spawn at
   the root's configured maximum depth.
2. Use that predicate in tool planning and every V2 spawn path owned by this change.
3. Capture the root value in tree-shared agent control before child role/config layers are finalized,
   and normalize descendants to that value.
4. Add handler, planner, and end-to-end coverage for the exact reported configuration and the
   role-layer invariant.

### Scope

In scope:

- direct MultiAgent V2 thread-spawn depth enforcement;
- matching model-visible `spawn_agent` exposure;
- in-scope CSV fan-out admission;
- root ownership across role config layering; and
- behavioral integration tests using `[agents].max_depth`.

Out of scope:

- a new `features.multi_agent_v2.max_depth` key or a changed default/range;
- derive/fork paths not owned by this spawn admission change (#30692);
- sandbox narrowing and worktree/write-scope isolation;
- custom-agent schema exposure and model catalog policy (#32782);
- capacity/residency preflight (#23479, #32353);
- route persistence and canonical activity metadata (#26718, #32504);
- requester-aware nested completion delivery (#32203); and
- TUI/Desktop orchestration UX.

### User-visible behavior

| Configuration | Root | Depth-1 child | Depth-2 child |
|---|---|---|---|
| `max_depth = 1` | May spawn | Cannot spawn; tool omitted | Not reachable |
| `max_depth = 2` | May spawn | May spawn | Cannot spawn; tool omitted |

An explicit over-depth replay/direct call fails before thread creation. Agent management tools remain
available at the leaf so the agent can message, wait for, or inspect already known agents as allowed.

### Test plan

- Exact regression:
  - load `[agents].max_depth = 1` through the normal config path;
  - root -> child succeeds;
  - child -> grandchild fails before thread creation.
- Tool planning:
  - root exposes `spawn_agent`;
  - an eligible orchestrator exposes it at `max_depth = 2`;
  - a maximum-depth leaf omits it but retains management tools.
- Runtime admission:
  - root -> child -> grandchild succeeds at `max_depth = 2`;
  - the grandchild cannot create depth 3;
  - in-scope CSV fan-out follows the same rule.
- Policy ownership:
  - child roles attempting lower and higher depth values both retain the root's cap.
- Cross-platform:
  - use existing `test_codex` integration helpers so coverage remains valid on Linux, macOS, and
    Windows.

### Validation before marking ready

Run from `codex-rs/` after rebasing onto current upstream `main`:

```text
just test -p codex-core multi_agent_v2_spawn_agent_rejects_beyond_configured_depth
just test -p codex-core multi_agent_v2_depth_limited_grandchild_keeps_only_management_tools
just test -p codex-core v2_grandchild_request_omits_spawn_agent_at_depth_limit
just test -p codex-core multi_agent_v2_role_cannot_override_root_depth_limit
just test -p codex-core
just test
just fix -p codex-core
just fmt
git diff --check
```

The exact test names above come from the reference implementation and should be retained or updated
with the final port. Because this slice changes `codex-core`, run the complete workspace suite after
the crate-specific tests. Record any environment-only failures precisely. Do not rerun tests after
the final `fix` and `fmt`, and do not mark the PR ready until required checks pass and the branch is
current with upstream `main`.

### Reference implementation

- Fork PR: [jessatech/codex-dev#1](https://github.com/jessatech/codex-dev/pull/1)
- Squash commit: [`70136efb`](https://github.com/jessatech/codex-dev/commit/70136efbb6672c41ca987f9d1365a95d37405e8f)
- Depth implementation: [`1d47fae0`](https://github.com/jessatech/codex-dev/commit/1d47fae00052eb42b2ee6a066f1acaca6a32ad84)
- Root-policy correction: [`0d76468b`](https://github.com/jessatech/codex-dev/commit/0d76468b918deff4785a5a7c40086143d0c98f38)

Relevant proof points in the squash:

- [shared depth predicate](https://github.com/jessatech/codex-dev/blob/70136efbb6672c41ca987f9d1365a95d37405e8f/codex-rs/core/src/agent/registry.rs#L71-L97)
- [V2 admission](https://github.com/jessatech/codex-dev/blob/70136efbb6672c41ca987f9d1365a95d37405e8f/codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs#L57-L66)
- [model-visible tool gating](https://github.com/jessatech/codex-dev/blob/70136efbb6672c41ca987f9d1365a95d37405e8f/codex-rs/core/src/tools/spec_plan.rs#L800-L837)
- [root-owned cap](https://github.com/jessatech/codex-dev/blob/70136efbb6672c41ca987f9d1365a95d37405e8f/codex-rs/core/src/agent/control.rs#L95-L142)
- [normalization before session configuration](https://github.com/jessatech/codex-dev/blob/70136efbb6672c41ca987f9d1365a95d37405e8f/codex-rs/core/src/session/mod.rs#L541-L550)
- [role override regression](https://github.com/jessatech/codex-dev/blob/70136efbb6672c41ca987f9d1365a95d37405e8f/codex-rs/core/src/tools/handlers/multi_agents_tests.rs#L773-L858)

The reference fork uses a separate V2 depth key with a 1-4 range and default 2. This upstream draft
deliberately ports only the admission/root-ownership design while honoring the existing
`[agents].max_depth` contract from #32027. A dedicated V2 key or changed default belongs in a
maintainer-approved follow-up enhancement.

The squash contains unrelated route/provenance changes. These links are evidence and porting aids,
not a request to merge or cherry-pick the squash wholesale.

Fixes #32027

Refs #30692

Refs #32100
