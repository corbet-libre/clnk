# clnk: Link

The sole owner of device connections to the Forum: challenge/session binding, attendance, heartbeat and bounded feed routing. It owns no profile keys, guard verdicts, policy, contacts or alternate forum connection.

Implementation is in progress. No integration or coverage acceptance is claimed.

Link now holds the actual generated Forum client and keeps failed/cancelled
challenge attempts offline. Forum currently has no admission-success type;
Link exposes that typed unavailability. The transport conformance and live
entry/session integrations remain pending. [Contract](docs/CONTRACT.md).

## Scope

### Purpose

`clnk` is the device's single connection to `cfrm`, handling challenge and session binding, attendance publication, heartbeat, reconnect, and bounded feed routing.

### Owns

- The one generated `cfrm` client per device and community runtime.
- Presence as a heartbeat that renews an expiring entry; online means present.
- Entry publication and replacement, match-feed paging and polling, ciphertext reads, and room ordering, relay and pass requests for Attend and Groups.
- A fresh generation on every reconnect.
- Member-to-server traffic over HTTPS directly.

### Never

- Lets another facade open its own forum connection.
- Hides offline state, revives old presence, or treats cached matches as current after reconnect.
- Supplies private profile keys to `cfrm`.
- Routes forum traffic over Tor; that option is reserved for later.

### States

Disconnected, Challenged, Entering, and Present; changes update the entry; disconnect or lost liveness makes it Absent; rejected credentials or epochs require the lobby.

### Test obligations

- One connection per community and device runtime.
- A failed entry never appears online; a stale response cannot restore presence.
- Reconnect revalidates the current charter, credential, and publication.
- Going offline withdraws all Board state and ends live chats.
- A newer device session replaces older attendance under race, expiry, and stale disconnect; cancellation leaves the device offline.
- Identical native and Wasm test vectors, full line and branch coverage with real round trips, and fault injection covering delay, duplication, loss, cancellation, clock regression, and corruption.
- Community isolation, bounded work, no identifiers or plaintext in errors, no own crypto, and no personal identifiers in outbound requests.
