# Storage Migration

No `migrate_state`, `get_state`, `StateV1`, or `StateV2` entrypoint/type is
implemented by `contracts/escrow/src/lib.rs`.

The current contract stores live records under the `DataKey` variants in
`contracts/escrow/src/types.rs`, primarily `Contract(id)`, `MilestoneReleased`,
admin/pause/emergency keys, and reputation keys.

## Planned

Forward-compatible storage migration remains planned work. Until a dedicated
implementation issue exists, the documentation drift is tracked by
[#341](https://github.com/Talenttrust/Talenttrust-Contracts/issues/341).
