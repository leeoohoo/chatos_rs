#!/usr/bin/env node

const fs = require('fs');

let input = '';
let finished = false;

function finish() {
  if (finished) return;
  finished = true;
  let event = 'SessionStart';
  try {
    const payload = JSON.parse(input || '{}');
    event = payload.hook_event_name || payload.hookEventName || event;
  } catch (_) {}

  const additionalContext = [
    'PLAN LOOP ACTIVE.',
    'Before Main Chat finishes, call decide_task_route once. Choose direct_response only when the request can be fully answered without durable Task work; choose durable_task for plans, project investigation, files, commands, implementation, Plugins, or tracked execution. This is a semantic decision, never a keyword match. Follow the Host-provided next_action.',
    'If the request needs durable Task work, create exactly one planning Task first with task_kind=planning. Do not create execution Tasks until the Host supplies a confirmed requirement_plan.',
    'The planning Task must investigate available project evidence, use its Host-provided Requirement Survey capability for genuine human decisions, never fall back to generic Ask User forms, and publish the plan with requirement_plan_save. Native Requirement Survey and requirement_plan are authoritative; do not attach an optional authoring or persistence Plugin unless the user explicitly selected it as part of the deliverable.',
    'After confirmation, continue the same workflow from the Host-bound plan. During execution, resolve technical problems locally, retry with a materially different safe method, or revise the plan before asking the user.',
    'A declared capability that is unavailable at runtime is replan_required, not user permission work. If an optional Plugin was removed and is not an explicit deliverable, replan onto native capabilities instead of restoring it. For a genuine product decision, Main Chat creates a Requirement Survey and resumes the same Turn; never end with a vague blocked message. Ask the user only for credentials, permission, a product decision, an external human action, or review of an external side effect whose result is unknown.'
  ].join('\n');

  fs.writeSync(1, JSON.stringify({
    hookSpecificOutput: {
      hookEventName: event,
      additionalContext
    }
  }));
}

process.stdin.setEncoding('utf8');
process.stdin.on('data', chunk => { input += chunk; });
process.stdin.on('end', () => { finish(); process.exit(0); });
process.stdin.on('error', () => { finish(); process.exit(0); });
setTimeout(() => { finish(); process.exit(0); }, 1000);
