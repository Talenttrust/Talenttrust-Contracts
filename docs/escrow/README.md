# Escrow Contract Fee Model

This module adds configurable protocol fee settings for the escrow milestone model.

## New features

- `protocol_fee_bps` configurable in `create_contract` (0-10000 basis points).
- `protocol_fee_account` set at creation time; only this account can withdraw fees and update fee rate.
- Per-milestone fee accounting via `Milestone.protocol_fee` and `EscrowContract.protocol_fee_accrued`.
- `get_protocol_fee_accrued` to query current fee balance.
- `withdraw_protocol_fees` for controlled withdrawal.
- `set_protocol_fee_bps` to update protocol fee rate with authorization.

## Security controls

- Only the `protocol_fee_account` can adjust fee rate or withdraw accrued fees.
- Fee account is authenticated with `caller.require_auth()`.
- Fee bounds enforced at 0..=10000.
- All protocol fee operations use persisted state and safe integer arithmetic.

## Behaviour on release

On each milestone release:
- Compute fee: `milestone.amount * protocol_fee_bps / 10000`.
- Save fee to milestone object.
- Increment `protocol_fee_accrued`.
- Mark milestone released and contract status completed when all milestones done.
