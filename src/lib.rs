//! The sole device-side Forum client holder. Forum owns its wire registry and
//! authority. No stale reply, failed challenge or cancelled attempt creates
//! presence. Current server admission ports are still unavailable.
pub use cfrm::{ChallengeInput, ErrorCode, RoomInput, Transport};
use std::{cell::Cell, convert::Infallible};

/// Maximum encoded credential accepted for one challenge attempt.
pub const MAX_CREDENTIAL_BYTES: usize = 64 * 1024;
/// Bounded coarse Forum response. No profile page is decoded by this foundation.
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;
/// Combined opaque pass and command bytes accepted for one room request.
pub const MAX_ROOM_INPUT_BYTES: usize = 64 * 1024;

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

/// An attached anonymous generated client. Its transport is never exposed to
/// children and is never used for authenticated presence or forum actions.
pub struct AnonymousRooms<T> {
    client: cfrm::Client<T>,
}

/// One non-cloneable Link owns Forum clients for one runtime. Authentication-only
/// construction has no room methods; attach an independent anonymous transport
/// to obtain them. Children never receive either generated client.
pub struct Link<T, R = ()> {
    client: cfrm::Client<T>,
    rooms: R,
    state: Cell<State>,
}
impl<T: cfrm::Transport> Link<T> {
    /// The runtime supplies its single exact-origin HTTPS transport capability.
    pub fn new(transport: T) -> Self {
        Self {
            client: cfrm::Client::new(transport, MAX_RESPONSE_BYTES),
            rooms: (),
            state: Cell::new(State::Disconnected),
        }
    }
    /// Attach the runtime's separate exact-origin anonymous transport. The
    /// runtime must supply an isolated session without member cookies, tokens or
    /// identifiers; separate Rust values alone do not prove network unlinkability.
    /// This consumes the authentication-only Link, so a second room client cannot
    /// be attached to the resulting value.
    pub fn with_anonymous_rooms<R: Transport>(self, transport: R) -> Link<T, AnonymousRooms<R>> {
        Link {
            client: self.client,
            rooms: AnonymousRooms {
                client: cfrm::Client::new(transport, MAX_RESPONSE_BYTES),
            },
            state: self.state,
        }
    }
}
impl<T: cfrm::Transport, R> Link<T, R> {
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

impl<T: Transport, R: Transport> Link<T, AnonymousRooms<R>> {
    /// Route the owner's opaque ordering request over the anonymous session.
    pub async fn room_order(&self, input: RoomInput) -> Result<Infallible, ErrorCode> {
        self.room(input, cfrm::Request::RoomOrder).await
    }
    /// Route encrypted room traffic over the anonymous session.
    pub async fn room_relay(&self, input: RoomInput) -> Result<Infallible, ErrorCode> {
        self.room(input, cfrm::Request::RoomRelay).await
    }
    /// Route the owner's opaque resume request over the anonymous session.
    pub async fn room_resume(&self, input: RoomInput) -> Result<Infallible, ErrorCode> {
        self.room(input, cfrm::Request::RoomResume).await
    }
    /// Route the owner's opaque handover request over the anonymous session.
    pub async fn room_handover(&self, input: RoomInput) -> Result<Infallible, ErrorCode> {
        self.room(input, cfrm::Request::RoomHandover).await
    }
    async fn room(
        &self,
        input: RoomInput,
        request: fn(RoomInput) -> cfrm::Request,
    ) -> Result<Infallible, ErrorCode> {
        if input.room_pass.len().saturating_add(input.command.len()) > MAX_ROOM_INPUT_BYTES {
            return Err(ErrorCode::TooLarge);
        }
        self.rooms.client.call(request(input)).await
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
