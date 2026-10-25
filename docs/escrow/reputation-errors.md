# Reputation Error Codes

This document lists all error codes returned by the reputation contract, their causes, and how to resolve them.

## Error Codes

### `ContractNotFound`
- **When it fires:** The supplied `contract_id` is `0`, is greater than or equal to the next unallocated contract id, or no contract exists at that id.
- **How to avoid it:** Use the id returned by `create_contract`. Contract ids start at `1 and are allocated sequentially.
- **Found in entrypoints:** `issue_reputation`, `get_reputation_comment`, `submit_work_evidence`, `get_work_evidence`, `raise_dispute`, `resolve_dispute`.

### `U`authorizedRole`
- **When it fires:** The caller is not the contract client, or the contract is degenerate (client == freelancer).
- **How to avoid it:** Only the contract client may issue reputation, and the client must differ from the freelancer.
- **Found in entrypoints:** `issue_reputation`.

### `SelfRating`
- **When it fires:** `contract.client == contract.freelancer`.
- **How to avoid it:** Ensure the client and freelancer addresses are distinct when creating the contract.
- **Found in entrypoints:** `issue_reputation`.

### `NotCompleted`
- -**When it fires:** The contract is not in `Completed` status, or the freelancer has no pending reputation credit.
- **How to avoid it:** Only issue reputation after the contract has been fully released and the pending credit has been granted.
- **Found in entrypoints:** `issue_reputation`.

### `InvalidRating`
- **When it fires:** The rating is outside the configured inclusive range.
- **How to avoid it:** Pass a rating between the configured `min_rating` and `max_rating`.
- **Found in entrypoints:** `issue_reputation`.

### `EmptyComment`
- -**When it fires:** The comment has zero bytes.
- **How to avoid it:** Provide a non-empty comment string.
- **Found in entrypoints:** `issue_reputation`.

### `CommentTooLong`
- **When it fires:** The comment exceeds the configured byte limit.
- **How to avoid it:** Keep the comment within the configured `max_comment_bytes` value.
- **Found in entrypoints:** `issue_reputation`.

### `ReputationAlreadyIssued`
- **When it fires:** Reputation has already been issued for the contract.
- **How to avoid it:** Do not call `issue_reputation` more than once per contract.
- **Found in entrypoints:** `issue_reputation`.

### `InvalidProtocolParameters`
- **When it fires:** `reputation_config` values fall outside the protocol bounds (min rating >= 1, max rating <= 10, max rating >= min rating, comment bytes in [1, 1000]).
- **How to avoid it:** Pass values within the documented protocol bounds.
- **Found in entrypoints:** `set_reputation_config`.

#### `PotentialOverflow`
- **When it fires:** An arithmetic operation on reputation aggregates or pending credits would overflow or underflow.
- **How to avoid it:** This indicates a corrupted or extremely large state; contact maintainers.
- **Found in entrypoints:** `issue_reputation`, `grant_pending_reputation_credit`.
