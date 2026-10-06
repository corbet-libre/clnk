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

## Unreleased runtime successor

The integration candidate adds actual challenge/enter and per-request signing
ports through Link, with injected time and original Forum wire types. Native and
browser HTTPS transports are owned by Forum and reexported through Link. Link
keeps no device signing key, clears cancelled/expired/refused attendance and does
not retry ambiguous effects. Graceful departure uses an actual signed Depart;
`disconnect` clears local state and leaves an unreachable server lease to expire.
Board rechecks received original publications through Search before returning a
page, and checks exact Profile revision/ciphertext binding on retrieval. New paths
are uncompiled pending owner main releases; no complete coverage or live delivery
is claimed. Incremental delta handling, Mesh key-release composition and complete
native/browser qualification remain integration work.

The anonymous-room successor retains the original Assembly `PreparedPass` and
`ClientPass` privately, never raw connection signing keys. `prepare_room_epoch`
and `receive_room_epoch` use actual admitted device requests; only that authenticated
response selects the common descriptor and incarnation. `prepare_room_pass` and
`receive_room_pass` retain bounded exact-operation blinding state across ambiguous
transport outcomes and finalize only the original authenticated signature.
`execute_room` proves an original Assembly command with the retained opaque pass
and sends only command/proof over the distinct anonymous client. It never copies
the attendance session or device signature into that transport. Common descriptor
replacement or `close_rooms` drops old signers. Epoch expiry refuses local use.
Actual Axum/Charter/Profile/blind issuance/Assembly integration is covered by a
written Board test, not an executed release claim; browser execution is pending.
