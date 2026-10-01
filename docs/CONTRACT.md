# Link contract

Board owns one non-cloneable Link for a device/community runtime. Link contains
the Forum's actual generated `Client<T>` and reuses `ChallengeInput`, `Request`
and `ErrorCode`; it owns no copied action table, endpoint schema or verifier.
The embedding runtime supplies one exact-origin HTTPS transport capability.

The current generated Forum response has no success constructor. `connect`
therefore reports a typed refusal and can never claim presence. Empty/overlarge
credentials are refused before transport, responses are bounded, cancellation
clears the attempt, and a server's missing authority is visible as Unavailable.
Disconnect clears local state and never resurrects an old attendance generation.

Implemented: generated-client ownership and failure/cancellation lifecycle.
Pending: current challenge/session/G1/G2/G3 success ports, committed publication
entry/replacement, liveness, fresh reconnect generation, pages/deltas/ciphertext
and room operations. No caller-supplied session or fixture-success route bypasses
those missing owners. Private profile keys have no Forum input in Link.

Tests execute the real Forum registry in-process and exercise cancellation and
transport refusal natively and in actual Wasm. They establish local client
plumbing only, not HTTPS interoperability, current admission or end-to-end
behavior. Real HTTPS/device authority evidence remains required for acceptance.
