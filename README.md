# clnk: Link

The single device connection to the Forum: challenge/session binding, attendance, heartbeat and bounded feed routing. It owns no profile keys, guard verdicts, policy, contacts or alternate forum connection.

Implementation is in progress. No integration or coverage acceptance is claimed.

Link now holds the actual generated Forum client and keeps failed/cancelled
challenge attempts offline. Forum currently has no admission-success type;
Link exposes that typed unavailability. The transport conformance and live
entry/session integrations remain pending. [Contract](docs/CONTRACT.md).
