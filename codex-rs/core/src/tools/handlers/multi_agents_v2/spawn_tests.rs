//! Unit tests for spawn-agent fork-mode resolution.
//!
//! Encodes the fork-resolution rows of `test-matrix/routing-matrix.csv` (R01-R07, R16) at the
//! `SpawnAgentArgs::fork_mode` level. The regression under repair: an omitted `fork_turns`
//! combined with an explicit route (`agent_type`/`model`/`reasoning_effort`) must resolve to a
//! fresh child, not a full-history fork that then rejects the very overrides that were requested.

use super::SpawnAgentArgs;
use crate::agent::control::SpawnAgentForkMode;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;

fn spawn_args(
    agent_type: Option<&str>,
    model: Option<&str>,
    reasoning_effort: Option<ReasoningEffort>,
    fork_turns: Option<&str>,
) -> SpawnAgentArgs {
    SpawnAgentArgs {
        message: "hello".to_string(),
        task_name: "probe".to_string(),
        agent_type: agent_type.map(str::to_string),
        model: model.map(str::to_string),
        reasoning_effort,
        service_tier: None,
        fork_turns: fork_turns.map(str::to_string),
        fork_context: None,
    }
}

#[test]
fn omitted_fork_without_routing_override_is_full_history() {
    // R01: no explicit route + omitted fork => inherited full-history child.
    let mode = spawn_args(None, None, None, None)
        .fork_mode()
        .expect("valid fork mode");
    assert_eq!(mode, Some(SpawnAgentForkMode::FullHistory));
}

#[test]
fn omitted_fork_with_explicit_agent_type_is_fresh() {
    // R02 (regression): base collapses omission to full-history, then rejects the agent_type.
    let mode = spawn_args(Some("routing_probe"), None, None, None)
        .fork_mode()
        .expect("valid fork mode");
    assert_eq!(mode, None);
}

#[test]
fn omitted_fork_with_explicit_model_is_fresh() {
    // R05 (regression).
    let mode = spawn_args(None, Some("gpt-5.6-terra"), None, None)
        .fork_mode()
        .expect("valid fork mode");
    assert_eq!(mode, None);
}

#[test]
fn omitted_fork_with_explicit_effort_is_fresh() {
    // R06 (regression).
    let mode = spawn_args(None, None, Some(ReasoningEffort::Medium), None)
        .fork_mode()
        .expect("valid fork mode");
    assert_eq!(mode, None);
}

#[test]
fn explicit_fork_none_is_fresh() {
    // R03 / R07.
    let mode = spawn_args(Some("routing_probe"), None, None, Some("none"))
        .fork_mode()
        .expect("valid fork mode");
    assert_eq!(mode, None);
}

#[test]
fn explicit_fork_all_stays_full_history_even_with_route() {
    // R04: explicit full-history semantics are preserved; the handler still rejects the
    // overrides downstream via `reject_full_fork_spawn_overrides`.
    let mode = spawn_args(Some("routing_probe"), None, None, Some("all"))
        .fork_mode()
        .expect("valid fork mode");
    assert_eq!(mode, Some(SpawnAgentForkMode::FullHistory));
}

#[test]
fn positive_integer_fork_is_partial_history() {
    // R16.
    let mode = spawn_args(Some("routing_probe"), None, None, Some("3"))
        .fork_mode()
        .expect("valid fork mode");
    assert_eq!(mode, Some(SpawnAgentForkMode::LastNTurns(3)));
}

#[test]
fn blank_fork_follows_omitted_intent() {
    // Blank is treated as omission under the current contract, so it becomes intent-aware:
    // fresh when a route is present, full-history otherwise.
    let with_route = spawn_args(Some("routing_probe"), None, None, Some("   "))
        .fork_mode()
        .expect("valid fork mode");
    assert_eq!(with_route, None);

    let without_route = spawn_args(None, None, None, Some(""))
        .fork_mode()
        .expect("valid fork mode");
    assert_eq!(without_route, Some(SpawnAgentForkMode::FullHistory));
}

#[test]
fn zero_and_garbage_fork_are_errors() {
    assert!(spawn_args(None, None, None, Some("0")).fork_mode().is_err());
    assert!(
        spawn_args(None, None, None, Some("abc"))
            .fork_mode()
            .is_err()
    );
}

#[test]
fn fork_context_is_rejected() {
    let mut args = spawn_args(None, None, None, None);
    args.fork_context = Some(true);
    assert!(args.fork_mode().is_err());
}
