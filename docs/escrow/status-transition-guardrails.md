# Escrow Contract Status Transition Guardrails

The escrow contract implements a strict status transition guardrail to avoid invalid workflows and unexpected states.

## Valid status transitions

- `Created` -> `Funded`
- `Funded` -> `Completed`
- `Funded` -> `Disputed`
- `Disputed` -> `Completed`

## Behavior

- `deposit_funds`: requires `Created`; transitions to `Funded`.
- `release_milestone`: requires `Funded`; final milestone release sets status to `Completed`.
- `dispute_contract`: requires `Funded`; transitions to `Disputed`.
- `disputed` state forbids milestone release while requiring explicit resolution logic before moving to `Completed` (or another allowed transition).

## Security

- Invalid transitions panic immediately.
- All status changes pass through `EscrowContract::transition_status`.
- Guardrails are expressed in `ContractStatus::can_transition_to` and enforced by `ContractStatus::assert_can_transition_to`.
