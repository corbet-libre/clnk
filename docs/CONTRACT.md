# Link contract

Board owns one non-cloneable Link for a device/community runtime. Link contains
the Forum's actual generated `Client<T>` and reuses `ChallengeInput`, `Request`
and `ErrorCode`; it owns no copied action table, endpoint schema or verifier.
The embedding runtime supplies the authenticated exact-origin HTTPS transport
capability. The authentication-only Link has no room methods. Consuming it with
`with_anonymous_rooms` attaches a second private owner-generated client and returns
`Link<Auth, AnonymousRooms<Rooms>>`; only that type offers room ordering, relay,
resume and handover. Each operation uses the actual Forum `RoomInput`, containing
only `room_pass` and `command`, and bounds their combined size before transport.
Authenticated credentials and session identifiers never enter those request types.

The runtime must provide a separate exact-origin anonymous HTTPS session without
member cookies, tokens or identifiers. Distinct Rust transport values establish
routing separation, not network unlinkability. Link cannot inspect an opaque
transport implementation or certify that its cookie/session storage is isolated.

The current generated Forum response has no success constructor. `connect`
therefore reports a typed refusal and can never claim presence. Empty/overlarge
credentials are refused before transport, responses are bounded, cancellation
clears the attempt, and a server's missing authority is visible as Unavailable.
Disconnect clears local state and never resurrects an old attendance generation.

Implemented: generated-client ownership, separate anonymous request routing,
input bounds and failure/cancellation lifecycle.
Pending: current challenge/session/G1/G2/G3 success ports, committed publication
entry/replacement, liveness, fresh reconnect generation, pages/deltas/ciphertext
and accepting room operations/blind-pass issuance. No caller-supplied session or fixture-success route bypasses
those missing owners. Private profile keys have no Forum input in Link.

Tests execute the real Forum registry in-process and exercise both client routes,
all four room request shapes, exact input bounds, cancellation and transport
refusal natively and in actual Wasm. An anonymous outage has no authenticated
fallback and cannot alter authenticated state. They establish local client
plumbing only, not HTTPS interoperability, current admission or end-to-end
behavior. Real HTTPS/device authority evidence remains required for acceptance.
