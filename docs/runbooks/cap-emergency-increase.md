# Cap emergency increase runbook

When to use
- Use only when the existing global contributor cap (percentage) is blocking legitimate contributors during an active wave and normal governance processes are too slow to address immediate needs.

Approval process
- Operator identifies need and contacts on-call admin and at least one senior approver.
- On-call admin must call `emergency_set_global_cap(admin, new_cap)` from the governance admin address; the admin call requires on-ledger auth.
- Log the decision, rationale, and approvers in the incident ticket and notify stakeholders.

Rollback procedure
- If the emergency cap causes problems, immediately call `emergency_set_global_cap(admin, old_cap)` to restore the previous value.
- After stabilization, file a normal governance proposal (non-emergency) to set a permanent cap and document the incident.

Monitoring & alerting
- Emit an alert if the global cap is changed more than twice within a rolling 24-hour window.
  - Detection: monitor contract events for `EmergencyCapUpdated` and `GlobalCapUpdated` topics.
  - Alert recipients: on-call admin, ops, and product owner.
- Record every cap change in an incident log with old/new values, admin actor, timestamp, and justification.

Implementation notes
- The emergency function bypasses the regular cap-change workflow and emits `EmergencyCapUpdated` for clear auditability.
- Cap values are constrained to 0..=100 percent to avoid invalid settings.

References
- On-chain function: `Escrow::emergency_set_global_cap(env, admin, new_cap)`
- Alerting: watch for more than two `*CapUpdated` events in 24h
