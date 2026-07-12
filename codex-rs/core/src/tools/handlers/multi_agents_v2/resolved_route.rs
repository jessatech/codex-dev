//! Structured, provenance-tagged record of how a spawned sub-agent's route resolved.
//!
//! The record is built from the child's *effective* configuration snapshot (the runtime source of
//! truth — never the child's self-report) compared against what the spawn call explicitly
//! requested, the parent baseline it would otherwise inherit, and whether a custom agent (role)
//! file was applied. It exists so operators — and, later, a strict-routing gate — can see and
//! verify exactly what each child consumed, and so a silent model/effort substitution surfaces as
//! data (`warnings`) instead of disappearing.

use crate::agent::control::SpawnAgentForkMode;
use codex_protocol::openai_models::ReasoningEffort;
use serde::Serialize;

/// Where a resolved routing value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RouteSource {
    /// Explicitly set on the `spawn_agent` tool call.
    ExplicitSpawnArgument,
    /// Set by the selected custom agent (role) file.
    CustomAgentFile,
    /// Inherited from the parent agent's effective config.
    ParentInheritance,
    /// Filled from the model catalog default (e.g. a model's default reasoning effort).
    ModelCatalogDefault,
    /// Filled from a client default (e.g. the default role name).
    ClientDefault,
}

/// A resolved value tagged with its provenance.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RoutedValue<T> {
    pub value: T,
    pub source: RouteSource,
}

impl<T> RoutedValue<T> {
    fn new(value: T, source: RouteSource) -> Self {
        Self { value, source }
    }
}

/// Serializable rendering of the resolved context-fork mode.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "turns")]
pub(crate) enum RoutedForkMode {
    Fresh,
    FullHistory,
    LastNTurns(usize),
}

impl RoutedForkMode {
    fn from_mode(mode: Option<&SpawnAgentForkMode>) -> Self {
        match mode {
            None => Self::Fresh,
            Some(SpawnAgentForkMode::FullHistory) => Self::FullHistory,
            Some(SpawnAgentForkMode::LastNTurns(turns)) => Self::LastNTurns(*turns),
        }
    }
}

/// What the spawn call explicitly requested, plus the resolved role context.
pub(crate) struct RouteRequest {
    pub task_name: String,
    /// Resolved role name (`agent_type` if provided, else the default role name).
    pub role_name: String,
    /// Whether `agent_type` was explicitly provided (vs defaulted).
    pub agent_type_explicit: bool,
    /// Path of the applied custom agent (role) file, if any.
    pub agent_config_path: Option<String>,
    pub requested_model: Option<String>,
    pub requested_reasoning_effort: Option<ReasoningEffort>,
    pub requested_service_tier: Option<String>,
    pub fork_mode: Option<SpawnAgentForkMode>,
}

/// The child's effective configuration, read from its runtime snapshot.
pub(crate) struct EffectiveRoute {
    pub model: String,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub service_tier: Option<String>,
}

/// The parent baseline the child would inherit absent any override.
pub(crate) struct ParentBaseline {
    pub model: String,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub service_tier: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolvedSubagentRoute {
    pub task_name: String,
    pub agent_type: RoutedValue<String>,
    pub model: RoutedValue<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<RoutedValue<ReasoningEffort>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<RoutedValue<String>>,
    pub fork_mode: RoutedForkMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_config_path: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl ResolvedSubagentRoute {
    /// Derives the resolved route and its provenance.
    ///
    /// Effective values come from the child's runtime snapshot. Provenance is derived by comparing
    /// each effective value against the explicit request, the parent baseline, and whether a role
    /// file was applied. A mismatch between an explicit request and the effective value is recorded
    /// as a warning (a silent substitution the caller should see).
    pub(crate) fn resolve(
        request: RouteRequest,
        effective: EffectiveRoute,
        parent: ParentBaseline,
    ) -> Self {
        let mut warnings = Vec::new();
        let role_applied = request.agent_config_path.is_some();

        let agent_type_source = if request.agent_type_explicit {
            RouteSource::ExplicitSpawnArgument
        } else {
            RouteSource::ClientDefault
        };
        let agent_type = RoutedValue::new(request.role_name, agent_type_source);

        let model_source = if let Some(requested) = request.requested_model.as_deref() {
            if requested != effective.model {
                warnings.push(format!(
                    "requested model `{requested}` but the child resolved to `{}`",
                    effective.model
                ));
            }
            RouteSource::ExplicitSpawnArgument
        } else if effective.model != parent.model {
            if role_applied {
                RouteSource::CustomAgentFile
            } else {
                RouteSource::ModelCatalogDefault
            }
        } else {
            RouteSource::ParentInheritance
        };
        let model = RoutedValue::new(effective.model, model_source);

        let reasoning_effort = effective.reasoning_effort.map(|effort| {
            let source = if let Some(requested) = request.requested_reasoning_effort.as_ref() {
                if *requested != effort {
                    warnings.push(format!(
                        "requested reasoning effort `{requested}` but the child resolved to `{effort}`"
                    ));
                }
                RouteSource::ExplicitSpawnArgument
            } else if Some(&effort) != parent.reasoning_effort.as_ref() {
                if role_applied {
                    RouteSource::CustomAgentFile
                } else {
                    RouteSource::ModelCatalogDefault
                }
            } else {
                RouteSource::ParentInheritance
            };
            RoutedValue::new(effort, source)
        });

        let service_tier = effective.service_tier.map(|tier| {
            let source = if let Some(requested) = request.requested_service_tier.as_deref() {
                if requested != tier {
                    warnings.push(format!(
                        "requested service tier `{requested}` but the child resolved to `{tier}`"
                    ));
                }
                RouteSource::ExplicitSpawnArgument
            } else if role_applied && Some(&tier) != parent.service_tier.as_ref() {
                RouteSource::CustomAgentFile
            } else {
                RouteSource::ParentInheritance
            };
            RoutedValue::new(tier, source)
        });

        Self {
            task_name: request.task_name,
            agent_type,
            model,
            reasoning_effort,
            service_tier,
            fork_mode: RoutedForkMode::from_mode(request.fork_mode.as_ref()),
            agent_config_path: request.agent_config_path,
            warnings,
        }
    }
}

#[cfg(test)]
#[path = "resolved_route_tests.rs"]
mod resolved_route_tests;
