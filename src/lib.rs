//! The sole device-side Forum client holder. Forum owns its wire registry and
//! authority. No stale reply, failed challenge or cancelled attempt creates
//! presence. Current server admission ports are still unavailable.
pub use cfrm::{ChallengeInput, ErrorCode, Transport};
use std::{cell::Cell, convert::Infallible};

/// Maximum encoded credential accepted for one challenge attempt.
pub const MAX_CREDENTIAL_BYTES: usize = 64 * 1024;
/// Bounded coarse Forum response. No profile page is decoded by this foundation.
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// Locally observable connection lifecycle, never inferred from TCP/HTTP success.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub enum State {
    /// No live forum attendance is held.
    Disconnected,
    /// A challenge request is in flight; no member information may be served yet.
    Connecting,
    /// Forum explicitly reports that required child authority is unavailable.
    Unavailable,
}

/// One non-cloneable generated client belongs to Board for one runtime. Children
/// use Board operations; they never receive or construct a second Forum client.
pub struct Link<T> {
    client: cfrm::Client<T>,
    state: Cell<State>,
}
impl<T: cfrm::Transport> Link<T> {
    /// The runtime supplies its single exact-origin HTTPS transport capability.
    pub fn new(transport: T) -> Self {
        Self {
            client: cfrm::Client::new(transport, MAX_RESPONSE_BYTES),
            state: Cell::new(State::Disconnected),
        }
    }
    /// No session or presence is fabricated while child admission is unavailable.
    pub fn state(&self) -> State {
        self.state.get()
    }
    /// Begin the owner-generated credential challenge. No copied request schema
    /// or additional session protocol exists here. The current Forum response
    /// has no success constructor; current G1/G2/G3 must mature before entry.
    pub async fn connect(&mut self, input: ChallengeInput) -> Result<Infallible, ErrorCode> {
        self.state.set(State::Disconnected);
        if input.credential.is_empty() {
            return Err(ErrorCode::InvalidRequest);
        }
        if input.credential.len() > MAX_CREDENTIAL_BYTES {
            return Err(ErrorCode::TooLarge);
        }
        self.state.set(State::Connecting);
        let mut attempt = Attempt {
            state: &self.state,
            finished: false,
        };
        let result = self.client.call(cfrm::Request::Challenge(input)).await;
        match result {
            Err(error) => {
                attempt
                    .state
                    .set(if error == ErrorCode::AuthorityUnavailable {
                        State::Unavailable
                    } else {
                        State::Disconnected
                    });
                attempt.finished = true;
                Err(error)
            }
            Ok(impossible) => match impossible {},
        }
    }
    /// Clear local refusal state. No attendance exists to revive or cache.
    pub fn disconnect(&mut self) {
        self.state.set(State::Disconnected);
    }
}
struct Attempt<'a> {
    state: &'a Cell<State>,
    finished: bool,
}
impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.state.set(State::Disconnected);
        }
    }
}
