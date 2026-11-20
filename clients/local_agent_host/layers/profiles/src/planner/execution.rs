#[async_trait]
impl LocalAiStepPlanner for ControlPlaneLocalAiStepPlanner {
    async fn prepare_ai_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<PreparedLocalAiStep, String> {
        if claim.run.profile_key != self.profile_key {
            return Err(format!(
                "planner for {} cannot execute profile {}",
                self.profile_key, claim.run.profile_key
            ));
        }
        let mut transient = self
            .model_resolver
            .resolve_model_runtime(
                &claim.run.owner_user_id,
                &claim.run.model_config_ref,
                &claim.run.model_config_revision,
            )
            .await?;
        transient.model_config.validate_model_capability()?;
        apply_run_thinking_level(&mut transient.model_config, &claim.run.input)?;
        let mut capabilities = self
            .capability_resolver
            .resolve_capabilities(
                &claim.run.owner_user_id,
                self.profile_key,
                &claim.run.capability_policy_revision,
            )
            .await?;
        capabilities.tools = merge_local_tools(
            capabilities.tools,
            &self.local_tools,
            &self.local_tool_prefixes,
        )?;
        if self.profile_key == TASK_EXECUTION_PROFILE_KEY {
            capabilities.tools = task_scoped_tools(capabilities.tools, &claim.run.input)?;
        } else if self.profile_key == MAIN_CHAT_PROFILE_KEY {
            capabilities.tools = main_chat_scoped_tools(capabilities.tools, &claim.run.input);
            capabilities.tools = scope_agent_direct_tools(capabilities.tools, &claim.run.input);
        }
        if claim.run.input.get("team_executor_context").is_none() {
            capabilities
                .prefixed_input_items
                .extend(self.local_prefixed_input_items.clone());
        }
        // Refresh even for resumed Tasks; page text is not the runtime clock.
        capabilities
            .prefixed_input_items
            .push(runtime_clock_context());
        capabilities
            .prefixed_input_items
            .push(bound_project_context(&claim.run.input));
        capabilities
            .prefixed_input_items
            .push(bound_remote_connection_context(
                &claim.run.input,
                self.profile_key == MAIN_CHAT_PROFILE_KEY,
            ));
        if let Some(provider) = self.hook_context_provider.as_ref() {
            capabilities
                .prefixed_input_items
                .extend(provider.prefixed_context(claim).await?);
        }
        let configured_instructions = merge_instructions(
            capabilities.instructions,
            transient.model_config.instructions,
        );
        let configured_instructions = merge_instructions(
            agent_direct_instructions(&claim.run.input)?,
            configured_instructions,
        );
        transient.model_config.instructions = if self.profile_key == TASK_EXECUTION_PROFILE_KEY {
            merge_instructions(
                Some(TASK_EXECUTION_WORKSPACE_INSTRUCTIONS.to_string()),
                configured_instructions,
            )
        } else {
            configured_instructions
        };
        let (mut current_input_items, reason) = durable_step_input(claim, self.initial_text_field)?;
        if transient.model_config.supports_images == Some(false) {
            // Do not let an image-producing tool turn a text-only model request
            // into an invalid gateway call or imply that this model saw pixels.
            for item in &mut current_input_items {
                if let Some(parts) = item.get_mut("content").and_then(Value::as_array_mut) {
                    parts.retain(|part| {
                        !matches!(
                            part.get("type").and_then(Value::as_str),
                            Some("input_image" | "image_url")
                        )
                    });
                    if parts.is_empty() {
                        parts.push(json!({"type":"input_text", "text":"Image input withheld: the configured model does not support images."}));
                    }
                }
            }
            capabilities.prefixed_input_items.push(message_item("system", Value::String(
                "[Model image capability] This Run's configured model explicitly does not support images. Image parts are not sent to this model. For image-dependent work, explain this specific limitation and ask for an image-capable model or an authorized OCR alternative. Do not claim to have seen an image or create a Task merely to bypass this limitation.".into()
            )));
        }
        if self.profile_key == TASK_EXECUTION_PROFILE_KEY
            && claim
                .run
                .input
                .get("acceptance_report_version")
                .and_then(Value::as_u64)
                == Some(1)
        {
            // Program-owned frozen data is available even after history truncation.
            capabilities.prefixed_input_items.push(message_item(
                "user",
                Value::String(
                    json!({
                        "type":"task_acceptance_contract",
                        "acceptance_report_version":1,
                        "expected_outputs":claim.run.input.get("expected_outputs"),
                        "acceptance_criteria":claim.run.input.get("acceptance_criteria"),
                        "final_response_max_chars":claim.run.input.get("final_response_max_chars"),
                        "source_user_message":claim.run.input.get("source_user_message"),
                        "requirements":claim.run.input.get("requirements"),
                        "requirement_refs":claim.run.input.get("requirement_refs"),
                        "unassigned_requirement_refs":claim.run.input.get("unassigned_requirement_refs"),
                        "guidance":"Implement and verify only the bound requirement_refs. Compare each interpreted requirement with its exact source_quote before writing. If they materially conflict, stop mutation and report a contract_conflict naming the requirement_id; do not silently choose one. Unassigned requirements are explicitly outside this Task."
                    })
                    .to_string(),
                ),
            ));
        }
        if self.profile_key == TASK_EXECUTION_PROFILE_KEY {
            capabilities.prefixed_input_items.push(message_item(
                "system",
                Value::String(json!({
                    "type": "task_run_convergence_contract",
                    "guidance": "Keep this Run compact. Reuse successful file reads and capability discovery already returned in this Run unless a later mutation invalidated them. Finish necessary reads and decide the change before opening an edit session; batch related files in one session and commit once per logical change. Reopen a committed file only for new evidence or a concrete verification failure. Use process_wait for a waitable long-running process; use polling only for incremental logs or interaction. If a tool returns duplicate_call_refused, summarize confirmed facts, remaining acceptance checks, and the next different minimal operation instead of retrying unchanged arguments."
                }).to_string()),
            ));
            if let Some(verification_contract) = claim
                .run
                .input
                .get("verification_contract")
                .filter(|value| !value.is_null())
            {
                capabilities.prefixed_input_items.push(message_item(
                    "system",
                    Value::String(json!({
                        "type": "task_verification_contract",
                        "verification_contract": verification_contract,
                        "guidance": "Verify the frozen subject_identity through the declared workflow_entry. Run every check whose declared dependencies have not failed or been skipped, even when another independent check fails. Skip only checks with failed or skipped direct dependencies and name those exact check_ids. Return every frozen check exactly once in verification_report. When require_full_rerun is true, rerun the entire declared workflow after any mutation before reporting success. Do not add project-specific checks that are absent from this contract."
                    }).to_string()),
                ));
            }
            if let Some(selected_plugins) = claim
                .run
                .input
                .pointer("/tool_options/plugin_hints")
                .and_then(Value::as_array)
                .filter(|plugins| !plugins.is_empty())
            {
                // Selection freezes tool scope, not a new output requirement. Repeat the
                // distinction after truncation: research tools need not produce artifacts,
                // while a deliverable explicitly requested in a Plugin still belongs there.
                capabilities.prefixed_input_items.push(message_item(
                    "system",
                    Value::String(
                        json!({
                            "type": "task_selected_plugin_contract",
                            "selected_plugins": selected_plugins,
                            "completion_basis": "task_objective_and_acceptance_criteria",
                            "guidance": "Use the selected Plugins for their stated purpose; discover, describe, activate, and invoke the relevant tools. Tool selection does not add deliverables to the Task. Determine completion from the objective, expected_outputs, and acceptance_criteria. For example, a browser used to research a text report supplies evidence; it does not make a persisted screenshot mandatory. If the Task permits HTTP fallback, a failed browser call does not prevent success when that fallback actually satisfies the criteria; disclose the failure and any visual evidence limits. In contrast, if the Task requests a screenshot or a document persisted in a selected Plugin, create and verify that deliverable there; prose, Notepad, an unrelated successful tool call, or an ordinary file cannot substitute for it. Report blocked only when an actual required outcome remains unmet and needs external action, not merely because an optional tool or artifact failed."
                        })
                        .to_string(),
                    ),
                ));
            }
        }
        if self.profile_key == MAIN_CHAT_PROFILE_KEY {
            let attachment_catalog = if let (Some(runtime), Some(conversation_id)) = (
                self.local_context_runtime.as_ref(),
                claim
                    .run
                    .input
                    .get("conversation_id")
                    .and_then(Value::as_str),
            ) {
                runtime
                    .recent_conversation_attachments(&claim.run.owner_user_id, conversation_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .map(|attachment| {
                        json!({
                            "attachment_id": attachment.attachment_id,
                            "turn_id": attachment.turn_id,
                            "display_name": attachment.display_name,
                            "media_type": attachment.media_type,
                            "byte_size": attachment.byte_size,
                            "sha256": attachment.sha256,
                            "authorized_local_ref": attachment.authorized_local_ref,
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            capabilities.prefixed_input_items.push(message_item(
                "system",
                Value::String(
                    json!({
                        "type": "task_context_compilation_contract",
                        "guidance": "Main Chat is the context curator for every Task. Before creating work, turn each requested outcome into a requirement with a stable id, an exact source_quote from an authoritative user message, an observable interpretation, and whether it requires change. The Host binds source message identity programmatically; never supply or invent a source message id. Bind every Task, expected output, and acceptance criterion to those requirement ids. A change requirement must have an implementation Task with write capability; a read-only verification Task cannot be its only owner. If the contract would add a business default, keep/delete choice, automatic/manual behavior, or data-overwrite policy absent from the user's words, clarify before dispatch; ordinary technical implementation choices do not require approval. Select only the prior facts, decisions, constraints, identifiers, paths, URLs, and verified results that the Task actually needs, then rewrite them as a self-contained context_summary. Never say 'see above', 'use prior chat', 'read Memory', or assume the Task can inspect Main Chat history. Do not dump the conversation or all retained Memory. Select only necessary attachments through attachment_refs using the opaque authorized_local_ref values in available_attachments; explain each selected attachment's purpose in context_summary. Never copy attachment bytes, base64, passwords, tokens, Authorization headers, or other secret values into Task arguments. If a required fact or safe reference is unavailable, resolve that gap before dispatch instead of creating an impossible Task.",
                        "available_attachments": attachment_catalog,
                        "attachment_catalog_scope": "recent_source_conversation_history",
                    })
                    .to_string(),
                ),
            ));
            capabilities.prefixed_input_items.push(message_item(
                "system",
                Value::String(
                    json!({
                        "type": "current_task_status_contract",
                        "guidance": "Conversation text and retained Memory are historical evidence, not authoritative current Task state. Before answering any question about what a Task is doing now, whether it completed, failed, blocked, was cancelled, or was replaced, call get_task or list_tasks. get_task resolves a historical id through its replacement chain; report current_tasks as the current state."
                    })
                    .to_string(),
                ),
            ));
            if let Some(selection) = claim.run.input.get("explicit_capability_selection") {
                let kind = selection.get("kind").and_then(Value::as_str);
                let contract = match kind {
                    Some("plugin") => Some(json!({
                        "type": "user_selected_capability_contract",
                        "selection": selection,
                        "required_capability": "selected_plugin",
                        "completion_basis": "task_objective_and_acceptance_criteria",
                        "guidance": "The user explicitly selected this Plugin in the composer. Create the requested durable Task with this exact Plugin for its requested purpose. The selected Plugin is required, not exclusive: complementary Plugins may be added when they perform distinct necessary work, but they must not replace the user's selected Plugin. The Host independently freezes and validates every selected component against the request-scoped catalog. Tool selection alone does not require a persisted artifact: browser research may deliver a sourced text report, whereas a document explicitly requested in a Plugin must be created and verified there, not substituted with Notepad or an ordinary file. Express the user's actual deliverable and evidence requirements in the objective and acceptance criteria; do not invent an artifact requirement merely because a Plugin is selected. Do not ask the user for a Plugin key or version, and do not claim completion in Main Chat."
                    })),
                    Some("requirement_survey") => Some(json!({
                        "type": "user_selected_capability_contract",
                        "selection": selection,
                        "delivery_route": "requirement_survey_direct",
                        "guidance": "The user explicitly selected Requirement Survey in the composer. Activate the requirement-discovery Skill and call requirement_survey_create directly in Main Chat. Do not create a survey placeholder Task and do not dispatch dependent work before the submitted answers and confirmed proposal."
                    })),
                    _ => None,
                };
                if let Some(contract) = contract {
                    capabilities
                        .prefixed_input_items
                        .push(message_item("system", Value::String(contract.to_string())));
                }
            }
        }
        if self.profile_key == TASK_EXECUTION_PROFILE_KEY {
            if let Some(instructions) = claim
                .run
                .input
                .get("retry_instructions")
                .and_then(Value::as_array)
                .filter(|instructions| !instructions.is_empty())
            {
                capabilities.prefixed_input_items.push(message_item(
                    "user",
                    Value::String(
                        json!({
                            "type": "task_retry_context",
                            "instructions": instructions,
                            "fresh_evidence_required": true,
                            "guidance": "This is a fresh Task retry. Treat conclusions and tool outputs from earlier Runs as historical only. Re-execute every read or validation needed to address the retry instructions. Do not repeat a previous blocker unless fresh evidence from this Run still demonstrates it."
                        })
                        .to_string(),
                    ),
                ));
            }
        }
        let pending_review = claim.run.input.get("task_followup_review");
        if let Some(plan) = claim.run.input.get("requirement_plan") {
            // Frozen program-owned context is sent on every step independently of
            // conversation memory truncation. Plan prose remains user/task data.
            capabilities.prefixed_input_items.push(message_item(
                "user",
                Value::String(
                    json!({"type": "requirement_plan_context", "plan": plan}).to_string(),
                ),
            ));
        }
        if let Some(review) = pending_review {
            // Program-owned facts only. New Runs get behavior from their frozen Product Skill.
            let mut authoritative_review = review.clone();
            if let Some(object) = authoritative_review.as_object_mut() {
                object.insert("context_authority".into(), json!("current_run"));
                object.insert(
                    "supersedes_all_prior_task_followup_reviews".into(),
                    json!(true),
                );
                object.insert(
                    "guidance".into(),
                    json!("This is the only authoritative current follow-up review. Ignore every older task_followup_review retained in conversation or checkpoint history. Copy this review_message_id and decide every task whose review_state is pending exactly once before creating or reusing work."),
                );
            }
            capabilities.prefixed_input_items.push(message_item(
                "system",
                Value::String(authoritative_review.to_string()),
            ));
        }
        let async_handoff_confirmed = self.profile_key == MAIN_CHAT_PROFILE_KEY
            && completed_async_task_handoff(claim.run.continuation_input.as_ref())
            && pending_review
                .is_none_or(|review| review.get("pending").and_then(Value::as_bool) != Some(true))
            && !claim.run.continuation_input.as_ref().is_some_and(|value| {
                value
                    .get("guidance")
                    .and_then(Value::as_array)
                    .is_some_and(|items| !items.is_empty())
            });
        let task_outcome_reported = self.profile_key == TASK_EXECUTION_PROFILE_KEY
            && (completed_task_outcome_report(
                claim.run.continuation_input.as_ref(),
                &claim.run.input,
            ) || task_acceptance_report_for_run(&claim.run).is_some_and(|report| {
                task_acceptance_report_is_terminal(&claim.run.input, report).unwrap_or(false)
            }));
        if async_handoff_confirmed {
            // Preserve the former server Task Runner boundary exactly: after the
            // explicit handoff succeeds, Main Chat gets one tool-free response
            // whose only job is to acknowledge that work has started. Task
            // results arrive later through the normal per-Task callbacks.
            capabilities.tools.clear();
            current_input_items.push(message_item(
                "system",
                Value::String(
                    json!({"type": "continued_work_accepted", "accepted": true}).to_string(),
                ),
            ));
        } else if task_outcome_reported {
            // Match the former Task Runner lifecycle hook: once the explicit
            // outcome has been accepted, the next request is a tool-free final
            // response. The model may explain the completed work, but it may
            // not perform more work or revise the reported terminal status.
            capabilities.tools.clear();
            current_input_items.push(message_item(
                "system",
                Value::String(
                    "[Task Outcome Reported]\nThe task outcome has been recorded. Tools are now disabled. Provide the final user-facing response based on the completed work and the reported outcome. Do not perform more work or revise the reported status."
                        .to_string(),
                ),
            ));
        }
        let mut memory = self
            .memory_source_id
            .as_deref()
            .map(|source_id| memory_plan(claim, source_id, self.initial_text_field))
            .transpose()?;
        if let Some(memory) = &mut memory {
            memory.scope.model_capability_revision =
                transient.model_config.model_capability_revision.clone();
            let available = transient
                .model_config
                .context_window_tokens
                .saturating_sub(transient.model_config.output_reserve_tokens);
            memory.scope.policy = Some(ComposeContextPolicy {
                max_memory_tokens: available,
                max_recent_record_tokens: available,
                max_thread_summary_tokens: available,
                max_project_memory_tokens: available,
                max_agent_memory_tokens: available,
            });
        }
        let conversation_id = memory
            .as_ref()
            .map(|memory| memory.thread_id.clone())
            .unwrap_or_else(|| claim.run.owner_entity_id.clone());
        let conversation_turn_id = memory
            .as_ref()
            .map(|memory| memory.turn_id.clone())
            .unwrap_or_else(|| claim.run.run_id.clone());
        let mut spec = RuntimeTurnSpec::new(transient.model_config, conversation_id)
            .with_conversation_turn_id(conversation_turn_id)
            .with_prefixed_input_items(capabilities.prefixed_input_items)
            .with_current_input_items(current_input_items)
            .with_tools(capabilities.tools);
        if let Some(memory) = &memory {
            spec = spec
                .with_memory_scope(Some(memory.scope.clone()))
                .with_record_options(memory.record_options.clone())
                .with_user_record(memory.user_record.clone());
        }
        let request = spec.into_contextual_turn_request();
        Ok(PreparedLocalAiStep {
            runner: transient.runner,
            request,
            reason,
            external_tool_results: memory
                .map(|memory| memory.external_tool_results)
                .unwrap_or_default(),
        })
    }
}
