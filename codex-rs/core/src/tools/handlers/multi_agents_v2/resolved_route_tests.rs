use super::EffectiveRoute;
use super::ParentBaseline;
use super::ResolvedSubagentRoute;
use super::RouteRequest;
use super::RouteSource;
use super::RoutedForkMode;
use crate::agent::control::SpawnAgentForkMode;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;
use serde_json::json;

fn parent() -> ParentBaseline {
    ParentBaseline {
        model: "gpt-5.6-sol".to_string(),
        reasoning_effort: Some(ReasoningEffort::High),
        service_tier: None,
    }
}

fn request(role_name: &str) -> RouteRequest {
    RouteRequest {
        task_name: "/root/probe".to_string(),
        role_name: role_name.to_string(),
        agent_type_explicit: role_name != "default",
        agent_config_path: None,
        requested_model: None,
        requested_reasoning_effort: None,
        requested_service_tier: None,
        fork_mode: None,
    }
}

fn effective(model: &str, effort: ReasoningEffort) -> EffectiveRoute {
    EffectiveRoute {
        model: model.to_string(),
        reasoning_effort: Some(effort),
        service_tier: None,
    }
}

#[test]
fn explicit_model_records_argument_source_without_warning() {
    let mut req = request("default");
    req.requested_model = Some("gpt-5.6-terra".to_string());
    req.fork_mode = None;
    let route = ResolvedSubagentRoute::resolve(
        req,
        effective("gpt-5.6-terra", ReasoningEffort::High),
        parent(),
    );

    assert_eq!(route.model.value, "gpt-5.6-terra");
    assert_eq!(route.model.source, RouteSource::ExplicitSpawnArgument);
    assert!(route.warnings.is_empty());
}

#[test]
fn substituted_role_values_report_role_provenance() {
    let mut req = request("routing_probe");
    req.agent_config_path = Some("/home/j/.codex/agents/routing_probe.toml".to_string());
    req.requested_reasoning_effort = Some(ReasoningEffort::High);
    req.requested_service_tier = Some("priority".to_string());
    let route = ResolvedSubagentRoute::resolve(
        req,
        EffectiveRoute {
            model: "gpt-5-role-override".to_string(),
            reasoning_effort: Some(ReasoningEffort::Minimal),
            service_tier: Some("flex".to_string()),
        },
        parent(),
    );

    assert_eq!(
        route.reasoning_effort.expect("effort present").source,
        RouteSource::CustomAgentFile
    );
    assert_eq!(
        route.service_tier.expect("service tier present").source,
        RouteSource::CustomAgentFile
    );
    assert_eq!(route.warnings.len(), 2);
}

#[test]
fn explicit_model_mismatch_is_flagged_as_silent_substitution() {
    let mut req = request("default");
    req.requested_model = Some("gpt-5.6-terra".to_string());
    let route = ResolvedSubagentRoute::resolve(
        req,
        effective("gpt-5.6-sol", ReasoningEffort::High),
        parent(),
    );

    assert_eq!(route.model.source, RouteSource::ParentInheritance);
    assert_eq!(route.warnings.len(), 1);
    assert!(route.warnings[0].contains("gpt-5.6-terra"));
    assert!(route.warnings[0].contains("gpt-5.6-sol"));
}

#[test]
fn role_supplied_model_records_custom_agent_file() {
    let mut req = request("routing_probe");
    req.agent_config_path = Some("/home/j/.codex/agents/routing_probe.toml".to_string());
    let route = ResolvedSubagentRoute::resolve(
        req,
        effective("gpt-5-role-override", ReasoningEffort::Minimal),
        parent(),
    );

    assert_eq!(route.model.source, RouteSource::CustomAgentFile);
    assert_eq!(route.agent_type.value, "routing_probe");
    assert_eq!(route.agent_type.source, RouteSource::ExplicitSpawnArgument);
    assert_eq!(
        route.agent_config_path.as_deref(),
        Some("/home/j/.codex/agents/routing_probe.toml")
    );
}

#[test]
fn inherited_model_records_parent_inheritance() {
    let route = ResolvedSubagentRoute::resolve(
        request("default"),
        effective("gpt-5.6-sol", ReasoningEffort::High),
        parent(),
    );

    assert_eq!(route.model.source, RouteSource::ParentInheritance);
    assert_eq!(route.agent_type.source, RouteSource::ClientDefault);
    let effort = route.reasoning_effort.expect("effort present");
    assert_eq!(effort.source, RouteSource::ParentInheritance);
}

#[test]
fn effort_changed_without_route_records_model_catalog_default() {
    // No explicit effort, no role file, but the effective effort differs from the parent — this is
    // the model's own default effort filling in.
    let route = ResolvedSubagentRoute::resolve(
        request("default"),
        effective("gpt-5.6-sol", ReasoningEffort::Medium),
        parent(),
    );
    let effort = route.reasoning_effort.expect("effort present");
    assert_eq!(effort.value, ReasoningEffort::Medium);
    assert_eq!(effort.source, RouteSource::ModelCatalogDefault);
}

#[test]
fn fork_mode_is_rendered_from_spawn_fork_mode() {
    let fresh = request("default");
    assert_eq!(
        ResolvedSubagentRoute::resolve(
            fresh,
            effective("gpt-5.6-sol", ReasoningEffort::High),
            parent()
        )
        .fork_mode,
        RoutedForkMode::Fresh
    );

    let mut partial = request("default");
    partial.fork_mode = Some(SpawnAgentForkMode::LastNTurns(3));
    assert_eq!(
        ResolvedSubagentRoute::resolve(
            partial,
            effective("gpt-5.6-sol", ReasoningEffort::High),
            parent()
        )
        .fork_mode,
        RoutedForkMode::LastNTurns(3)
    );
}

#[test]
fn serializes_camelcase_keys_and_snake_case_sources() {
    let mut req = request("routing_probe");
    req.agent_config_path = Some("/agents/routing_probe.toml".to_string());
    req.requested_model = Some("gpt-5.6-terra".to_string());
    req.fork_mode = Some(SpawnAgentForkMode::LastNTurns(3));
    let route = ResolvedSubagentRoute::resolve(
        req,
        effective("gpt-5.6-terra", ReasoningEffort::Medium),
        parent(),
    );
    let value = serde_json::to_value(&route).expect("route serializes");

    assert_eq!(value["taskName"], json!("/root/probe"));
    assert_eq!(value["agentType"]["value"], json!("routing_probe"));
    assert_eq!(
        value["agentType"]["source"],
        json!("explicit_spawn_argument")
    );
    assert_eq!(value["model"]["source"], json!("explicit_spawn_argument"));
    assert_eq!(
        value["forkMode"],
        json!({"kind": "last_n_turns", "turns": 3})
    );
    assert_eq!(
        value["agentConfigPath"],
        json!("/agents/routing_probe.toml")
    );
    // reasoning effort mismatch (requested none, effective medium) is not a warning; only explicit
    // requests warn. Here model matched, so no warnings key is serialized.
    assert!(value.get("warnings").is_none());
}
