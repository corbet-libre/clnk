# clnk engineering

The single device connection to the Forum: challenge/session binding, attendance, heartbeat and bounded feed routing. It owns no profile keys, guard verdicts, policy, contacts or alternate forum connection.

Reuse maintained upstream and authoritative owner ports. Never implement crypto,
policy or another leaf authority here. No fabricated success or mock-domain
acceptance. Device code requires native and executed wasm vectors.

First-party Git dependencies follow main, one locked revision per crate.
CI refreshes one snapshot, then uses --locked everywhere. Exact 100% reachable
line AND branch coverage; only reviewed justified unreachable exclusions.
No local Cargo, package publication, paid operations or attribution
trailers. Keep CI bounded. Every backend needs independent acceptance review.
