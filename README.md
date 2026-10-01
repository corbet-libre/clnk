# clnk: Link

The sole owner of device connections to the Forum: challenge/session binding, attendance, heartbeat and bounded feed routing. It owns no profile keys, guard verdicts, policy, contacts or alternate forum connection.

Implementation is in progress. No integration or coverage acceptance is claimed.

Link now holds the actual generated Forum client and keeps failed/cancelled
challenge attempts offline. Forum currently has no admission-success type;
Link exposes that typed unavailability. The transport conformance and live
entry/session integrations remain pending. [Contract](docs/CONTRACT.md).

## Scope

Link is the sole owner of every device connection to Forum for one community
runtime. It owns two unlinkable sessions: authenticated presence/forum traffic,
and anonymous public-room traffic using blind room passes. Connection ownership
is shared by neither other facades nor Board's children; room traffic must not
reveal a member-to-room link.

Link owns generated-client challenge/session binding, entry publication and
replacement, heartbeat, a fresh generation on reconnect, bounded page/delta routing,
ciphertext reads and anonymous room requests. Server traffic uses exact-origin
HTTPS. Profile keys never reach Forum. Link does not verify admission, implement
matching, conceal offline state or restore stale attendance/cached matches.

The intended lifecycle is disconnected, challenged, entering, present; changes
update the entry; lost liveness makes it absent; rejected credentials or epochs
require the lobby. Tests must cover failed entry, stale responses, reconnect with
current authority, replacement/expiry races, cancellation, complete local
withdrawal on disconnect and unlinkability of room and presence sessions.
Current generated-owner ports and remaining integration are listed in
[the contract](docs/CONTRACT.md).

`Link::with_anonymous_rooms` consumes the authentication-only Link and attaches a
separate generated client. Only the resulting type exposes the four owner room
requests; the owner DTO contains only pass and command fields. Link adds no
authenticated `Possession` field. Both classes still receive typed owner refusals. The embedding
runtime must establish actual HTTPS session unlinkability; these local routing
tests do not demonstrate it.
