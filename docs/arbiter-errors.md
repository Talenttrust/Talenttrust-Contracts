# Arbiter Error Codes

This document catalogs the `EscrowError` codes specifically related to the Arbiter role and dispute resolution in the Talent Trust escrow contracts.

| Code | Error Name | Fired By Entrypoint(s) | Trigger Condition | How to Avoid |
| ---- | ---------- | ---------------------- | ----------------- | ------------ |
| **25** | `ArbiterRequired` | `raise_dispute` | Fired when a client or freelancer attempts to open a dispute on a contract that was created without an assigned arbiter. | **How to avoid:** Ensure the contract is created with a valid `arbiter` address if you anticipate the need for dispute resolution. Contracts without arbiters cannot enter the `Disputed` state. |
| **26** | `InvalidDisputeSplit` | `resolve_dispute` | Fired when an arbiter attempts to resolve a dispute with a `Split` resolution, but the provided `client_amount` and `freelancer_amount` are invalid (e.g. negative, individually exceed the available balance, or do not sum exactly to the available balance). | **How to avoid:** The arbiter must compute the split such that `client_amount >= 0`, `freelancer_amount >= 0`, and `client_amount + freelancer_amount == available_balance` (where `available = funded - released - refunded`). |
| **35** | `MissingArbiter` | `create_contract` | Fired during contract creation if the chosen `ReleaseAuthorization` mode strictly requires an arbiter (such as `ArbiterOnly` or `ClientAndArbiter`), but the `arbiter` parameter was provided as `None`. | **How to avoid:** Always pass a valid `Some(Address)` for the `arbiter` parameter when initializing contracts with authorization modes that require an arbiter. |
| **36** | `InvalidArbiter` | `create_contract` | Fired during contract creation if the provided `arbiter` address is identical to either the `client` address or the `freelancer` address. | **How to avoid:** Ensure the arbiter is an independent third party. The escrow contract strictly enforces separation of concerns; an address cannot serve as both a principal (client/freelancer) and the arbiter for the same contract. |

> **Note:** The `UnauthorizedRole = 15` error code is also frequently encountered by arbiters if they attempt to call entrypoints restricted to the client or freelancer, or if a non-arbiter attempts to call `resolve_dispute`.


<!-- > **Note:** The `UnauthorizedRole = 15` error code is also frequently encountered by arbiters if they attempt to call entrypoints restricted to the client or freelancer, or if a non-arbiter attempts to call `resolve_dispute`. -->
