# Escrow Dispute Resolution

Dispute creation and dispute resolution are not implemented public entrypoints in
`contracts/escrow/src/lib.rs`.

Current cancellation is limited to `cancel_contract(contract_id, caller)`, where
the caller must be the stored client or freelancer and the contract must not be
completed or already cancelled.

Future dispute-resolution docs must link to the implementation issue and should
not list function signatures until those functions exist in `lib.rs`.
