# ContractSummary Schema Versioning

## Overview

`ContractSummary` is the immutable snapshot written into every `FinalizationRecord`.
Indexers read this struct to build off-chain representations of closed escrows.

The constant `CONTRACT_SUMMARY_SCHEMA_VERSION` (in `types.rs`) is stamped into
`ContractSummary.schema_version` at finalization time. Indexers store this value
alongside each record and call `get_contract_summary_schema_version()` to compare
against the on-chain version; a mismatch signals that re-processing is required.

## Current version

| Version | Status  | Notes                          |
|---------|---------|--------------------------------|
| 1       | Current | Initial layout                 |

## Bump policy

Increment `CONTRACT_SUMMARY_SCHEMA_VERSION` whenever a field is added, removed,
renamed, or changes meaning in a way that would silently produce wrong data in an
existing indexer:

1. Update `CONTRACT_SUMMARY_SCHEMA_VERSION` in `types.rs`.
2. Add a new row to the table above describing the change.
3. Add a migration note in the **Migration history** section below.
4. Update `docs/escrow/indexer-schema.md` if field names change.
5. Bump the associated test in `test/mainnet_readiness.rs` if the expected
   version constant changes.

Backwards-compatible additions (new *optional* output fields readable by old
indexers without error) do **not** require a bump.

## Migration history

_No migrations yet._
