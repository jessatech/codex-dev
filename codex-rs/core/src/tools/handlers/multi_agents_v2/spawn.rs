use super::resolved_route::EffectiveRoute;
use super::resolved_route::ParentBaseline;
use super::resolved_route::ResolvedSubagentRoute;
use super::resolved_route::RouteRequest;
use super::*;
use crate::agent::control::SpawnAgentForkMode;
use crate::agent::control::SpawnAgentOptions;
use crate::agent::next_thread_spawn_depth;
use crate::agent::role::DEFAULT_ROLE_NAME;
use crate::agent::role::apply_role_to_config;
use crate::agent::role::resolve_role_config;
use crate::agent::role::role_pinned_model_and_effort;
use crate::agent_communication::AgentCommunicationContext;
use crate::agent_communication::AgentCommunicationKind;
use crate::tools::handlers::multi_agents_spec::SpawnAgentToolOptions;
use crate::tools::handlers::multi_agents_spec::create_spawn_agent_tool_v2;
use crate::tools::handlers::multi_agents_v2::message_tool::message_content;
use codex_protocol::AgentPath;
use codex_tools::ToolSpec;

#[derive(Default)]
pub(crate) struct Handler {
    options: SpawnAgentToolOptions,
}

impl Handler {
    pub(crate) fn new(options: SpawnAgentToolOptions) -> Self {
        Self { options }
    }
}

impl ToolExecutor<ToolInvocation> for Handler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("spawn_agent")
    }

    fn spec(&self) -> ToolSpec {
        create_spawn_agent_tool_v2(self.options.clone())
    }

    fn handle(&self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'_> {
        Box::pin(async move { handle_spawn_agent(invocation).await.map(boxed_tool_output) })
    }
}

async fn handle_spawn_agent(
    invocation: ToolInvocation,
) -> Result<SpawnAgentResult, FunctionCallError> {
    let ToolInvocation {
        session,
        turn,
        payload,
        call_id,
        ..
    } = invocation;
    let arguments = function_arguments(payload)?;
    let args: SpawnAgentArgs = parse_arguments(&arguments)?;
    let fork_mode = args.fork_mode()?;
    let role_name = args
        .agent_type
        .as_deref()
        .map(str::trim)
        .filter(|role| !role.is_empty());

    if role_name.is_none() && turn.config.multi_agent_v2.require_explicit_agent_type {
        return Err(FunctionCallError::RespondToModel(
            "strict routing requires an explicit agent_type; specify a role instead of relying on \
             the inherited default"
                .to_string(),
        ));
    }

    if turn.config.multi_agent_v2.reject_route_substitution
        && let Some(role) =
            role_name.and_then(|name| resolve_role_config(turn.config.as_ref(), name))
    {
        let (pinned_model, pinned_effort) = role_pinned_model_and_effort(role);
        if let (Some(requested), Some(pinned)) = (args.model.as_deref(), pinned_model.as_deref())
            && requested != pinned
        {
            return Err(FunctionCallError::RespondToModel(format!(
                "strict routing: role `{}` pins model `{pinned}` and cannot honor the requested \
                 model `{requested}`",
                role_name.unwrap_or(DEFAULT_ROLE_NAME)
            )));
        }
        if let (Some(requested), Some(pinned)) =
            (args.reasoning_effort.as_ref(), pinned_effort.as_deref())
            && !requested.as_str().eq_ignore_ascii_case(pinned)
        {
            return Err(FunctionCallError::RespondToModel(format!(
                "strict routing: role `{}` pins reasoning effort `{pinned}` and cannot honor the \
                 requested effort `{}`",
                role_name.unwrap_or(DEFAULT_ROLE_NAME),
                requested.as_str()
            )));
        }
    }

    if let Some(cap) = turn.config.multi_agent_v2.max_total_spawns_per_root
        && session.services.agent_control.spawns_used() >= cap
    {
        return Err(FunctionCallError::RespondToModel(format!(
            "spawn budget reached: this root has already spawned {cap} agent(s) \
             (features.multi_agent_v2.max_total_spawns_per_root)"
        )));
    }

    let message = message_content(args.message)?;
    let session_source = turn.session_source.clone();
    let child_depth = next_thread_spawn_depth(&session_source);
    let mut config =
        build_agent_spawn_config(&session.get_base_instructions().await, turn.as_ref())?;
    if let Some(service_tier) = args.service_tier.as_ref() {
        config.service_tier = Some(service_tier.clone());
    }
    if matches!(fork_mode, Some(SpawnAgentForkMode::FullHistory)) {
        reject_full_fork_spawn_overrides(
            role_name,
            args.model.as_deref(),
            args.reasoning_effort.clone(),
        )?;
    } else {
        apply_requested_spawn_agent_model_overrides(
            &session,
            turn.as_ref(),
            &mut config,
            args.model.as_deref(),
            args.reasoning_effort.clone(),
        )
        .await?;
        apply_role_to_config(&mut config, role_name)
            .await
            .map_err(FunctionCallError::RespondToModel)?;
    }
    apply_spawn_agent_service_tier(
        &session,
        &mut config,
        turn.config.service_tier.as_deref(),
        args.service_tier.as_deref(),
    )
    .await?;
    apply_spawn_agent_runtime_overrides(&mut config, turn.as_ref())?;

    // Capture the requested route and parent baseline before `config` and `fork_mode` are consumed
    // by the spawn, so the effective route (read from the child snapshot below) can be reported
    // with provenance.
    let resolved_role_name = role_name.unwrap_or(DEFAULT_ROLE_NAME);
    let route_request = RouteRequest {
        task_name: args.task_name.clone(),
        role_name: resolved_role_name.to_string(),
        agent_type_explicit: role_name.is_some(),
        agent_config_path: resolve_role_config(turn.config.as_ref(), resolved_role_name)
            .and_then(|role| role.config_file.as_ref())
            .map(|path| path.display().to_string()),
        requested_model: args.model.clone(),
        requested_reasoning_effort: args.reasoning_effort.clone(),
        requested_service_tier: args.service_tier.clone(),
        fork_mode: fork_mode.clone(),
    };
    let parent_baseline = ParentBaseline {
        model: turn.model_info.slug.clone(),
        reasoning_effort: turn
            .reasoning_effort
            .clone()
            .or_else(|| turn.model_info.default_reasoning_level.clone()),
        service_tier: turn.config.service_tier.clone(),
    };

    let spawn_source = thread_spawn_source(
        session.thread_id,
        &turn.session_source,
        child_depth,
        role_name,
        Some(args.task_name.clone()),
    )?;
    let new_agent_path = spawn_source.get_agent_path().ok_or_else(|| {
        FunctionCallError::RespondToModel(
            "spawned agent is missing a canonical task name".to_string(),
        )
    })?;
    let author = turn
        .session_source
        .get_agent_path()
        .unwrap_or_else(AgentPath::root);
    let communication = communication_from_tool_message(author, new_agent_path.clone(), message);
    let context = AgentCommunicationContext::new(AgentCommunicationKind::Spawn, session.thread_id);
    let spawned_agent = Box::pin(
        session
            .services
            .agent_control
            .spawn_agent_with_communication(
                config,
                communication,
                context,
                Some(spawn_source),
                SpawnAgentOptions {
                    fork_parent_spawn_call_id: fork_mode.as_ref().map(|_| call_id.clone()),
                    fork_mode,
                    parent_thread_id: Some(session.thread_id),
                    environments: Some(turn.environments.to_selections()),
                },
            ),
    )
    .await
    .map_err(collab_spawn_error)?;
    let new_thread_id = spawned_agent.thread_id;
    session.services.agent_control.record_spawn();
    let agent_snapshot = session
        .services
        .agent_control
        .get_agent_config_snapshot(new_thread_id)
        .await;
    let nickname = agent_snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.session_source.get_nickname())
        .or(spawned_agent.metadata.agent_nickname);
    let resolved_route = agent_snapshot.as_ref().map(|snapshot| {
        ResolvedSubagentRoute::resolve(
            route_request,
            EffectiveRoute {
                model: snapshot.model.clone(),
                reasoning_effort: snapshot.reasoning_effort.clone(),
                service_tier: snapshot.service_tier.clone(),
            },
            parent_baseline,
        )
    });
    emit_sub_agent_activity(
        &session,
        &turn,
        SubAgentActivityItem {
            id: call_id,
            agent_thread_id: new_thread_id,
            agent_path: new_agent_path.clone(),
            kind: SubAgentActivityKind::Started,
        },
    )
    .await;
    let role_tag = role_name.unwrap_or(DEFAULT_ROLE_NAME);
    turn.session_telemetry.counter(
        "codex.multi_agent.spawn",
        /*inc*/ 1,
        &[("role", role_tag), ("version", "v2")],
    );
    let task_name = String::from(new_agent_path);

    let hide_agent_metadata = turn.config.multi_agent_v2.hide_spawn_agent_metadata;
    if hide_agent_metadata {
        Ok(SpawnAgentResult::HiddenMetadata { task_name })
    } else {
        Ok(SpawnAgentResult::WithNickname {
            task_name,
            nickname,
            route: resolved_route.map(Box::new),
        })
    }
}

impl CoreToolRuntime for Handler {
    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpawnAgentArgs {
    message: String,
    task_name: String,
    agent_type: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<ReasoningEffort>,
    service_tier: Option<String>,
    fork_turns: Option<String>,
    fork_context: Option<bool>,
}

impl SpawnAgentArgs {
    /// Resolves the requested context-fork mode.
    ///
    /// An explicit `fork_turns` is honored literally: `none` => fresh child, `all` => full
    /// history, positive integer => partial history. When `fork_turns` is omitted or blank the
    /// default is intent-aware: a spawn that explicitly requests a role, model, or reasoning
    /// effort resolves to a fresh child (so those overrides are honored instead of being rejected
    /// by `reject_full_fork_spawn_overrides`), while a spawn with no routing override keeps the
    /// inherited full-history default.
    fn fork_mode(&self) -> Result<Option<SpawnAgentForkMode>, FunctionCallError> {
        if self.fork_context.is_some() {
            return Err(FunctionCallError::RespondToModel(
                "fork_context is not supported in MultiAgentV2; use fork_turns instead".to_string(),
            ));
        }

        let explicit_fork_turns = self
            .fork_turns
            .as_deref()
            .map(str::trim)
            .filter(|fork_turns| !fork_turns.is_empty());

        let Some(fork_turns) = explicit_fork_turns else {
            let non_blank = |value: &Option<String>| {
                value
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|value| !value.is_empty())
            };
            let has_routing_override = non_blank(&self.agent_type)
                || non_blank(&self.model)
                || self.reasoning_effort.is_some();
            return Ok(if has_routing_override {
                None
            } else {
                Some(SpawnAgentForkMode::FullHistory)
            });
        };

        if fork_turns.eq_ignore_ascii_case("none") {
            return Ok(None);
        }
        if fork_turns.eq_ignore_ascii_case("all") {
            return Ok(Some(SpawnAgentForkMode::FullHistory));
        }

        let last_n_turns = fork_turns.parse::<usize>().map_err(|_| {
            FunctionCallError::RespondToModel(
                "fork_turns must be `none`, `all`, or a positive integer string".to_string(),
            )
        })?;
        if last_n_turns == 0 {
            return Err(FunctionCallError::RespondToModel(
                "fork_turns must be `none`, `all`, or a positive integer string".to_string(),
            ));
        }

        Ok(Some(SpawnAgentForkMode::LastNTurns(last_n_turns)))
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum SpawnAgentResult {
    WithNickname {
        task_name: String,
        nickname: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        route: Option<Box<ResolvedSubagentRoute>>,
    },
    HiddenMetadata {
        task_name: String,
    },
}

impl ToolOutput for SpawnAgentResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "spawn_agent")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, Some(true), "spawn_agent")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "spawn_agent")
    }
}

#[cfg(test)]
#[path = "spawn_tests.rs"]
mod spawn_tests;
