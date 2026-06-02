# Escrow Dispute Workflow

No dispute workflow is implemented in `contracts/escrow/src/lib.rs`.

The `ContractStatus::Disputed` enum variant exists, but there is no public
entrypoint that transitions a contract into or out of `Disputed`.

Dispute and refund designs should remain planned documentation until their
public entrypoints, tests, events, and storage records are implemented.
