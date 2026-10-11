//! The single member-side Forum connection. Signatures come from the device
//! owner; this library retains only bounded volatile handshake/session state.
pub use cfrm;
pub use cmbl;
mod rooms;
#[cfg(all(feature = "browser", target_arch = "wasm32"))]
pub use cfrm::BrowserTransport;
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub use cfrm::HttpTransport;
pub use cfrm::{ChallengeInput, ErrorCode, RoomInput, Transport};
pub use cmmr::Clock;
pub use rooms::OrderedRoom;
use std::cell::{Cell, RefCell};

pub const MAX_PUBLICATION_BYTES: usize = cswb::MAX_PUBLICATION_BYTES;
/// Retained source alias; the challenge now carries the original publication.
pub const MAX_CREDENTIAL_BYTES: usize = MAX_PUBLICATION_BYTES;
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
pub const MAX_ROOM_INPUT_BYTES: usize = 64 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub enum State {
    Disconnected,
    Connecting,
    Present,
    Unavailable,
}

struct Scope {
    community: String,
    origin: String,
    clock: Box<dyn Clock>,
    last: Cell<u64>,
}
impl Scope {
    fn now(&self) -> Result<u64, ErrorCode> {
        let now = self
            .clock
            .now()
            .map_err(|_| ErrorCode::AuthorityUnavailable)?;
        if now < self.last.get() || now > cmmr::MAX_TIME {
            return Err(ErrorCode::AuthorityUnavailable);
        }
        self.last.set(now);
        Ok(now)
    }
}
struct Pending {
    wire: Vec<u8>,
    publication: Vec<u8>,
    deadline: u64,
}
struct Attendance {
    state: State,
    session: Option<cfrm::EnterOutput>,
    pending: Option<Pending>,
    publication: Option<Vec<u8>>,
}
impl Attendance {
    fn clear(&mut self, state: State) {
        self.state = state;
        self.session = None;
        self.pending = None;
        self.publication = None;
    }
}
/// Exact bytes for a device-owned signer. This has no wire constructor and
/// does not grant attendance; finish the server challenge with its signature.
#[derive(PartialEq, Eq)]
pub struct Challenge {
    message: Vec<u8>,
}
impl std::fmt::Debug for Challenge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Challenge([REDACTED])")
    }
}
impl Challenge {
    pub fn message(&self) -> &[u8] {
        &self.message
    }
}
/// One original owner-generated request and its immutable signing transcript.
pub struct Prepared {
    request: cfrm::Request,
    message: Vec<u8>,
    session: Vec<u8>,
}
impl Prepared {
    pub fn request(&self) -> &cfrm::Request {
        &self.request
    }
    pub fn action(&self) -> &'static str {
        self.request.info().name
    }
    pub fn message(&self) -> &[u8] {
        &self.message
    }
}

pub struct AnonymousRooms<T> {
    client: cfrm::Client<T>,
    state: rooms::State,
}
pub struct Link<T, R = ()> {
    client: cfrm::Client<T>,
    rooms: R,
    scope: Option<Scope>,
    attendance: RefCell<Attendance>,
}
impl<T: Transport> Link<T> {
    /// An unconfigured client can observe refusal but cannot accept attendance.
    /// Production uses `at` with an independently configured scope and clock.
    pub fn new(transport: T) -> Self {
        Self {
            client: cfrm::Client::new(transport, MAX_RESPONSE_BYTES),
            rooms: (),
            scope: None,
            attendance: RefCell::new(Attendance {
                state: State::Disconnected,
                session: None,
                pending: None,
                publication: None,
            }),
        }
    }
    pub fn at(
        transport: T,
        community: String,
        origin: String,
        clock: impl Clock + 'static,
    ) -> Result<Self, ErrorCode> {
        cmmr::validate_name(&community).map_err(|_| ErrorCode::InvalidRequest)?;
        let parsed = url::Url::parse(&origin).map_err(|_| ErrorCode::Origin)?;
        if parsed.scheme() != "https" || parsed.origin().ascii_serialization() != origin {
            return Err(ErrorCode::Origin);
        }
        let mut link = Self::new(transport);
        link.scope = Some(Scope {
            community,
            origin,
            clock: Box::new(clock),
            last: Cell::new(0),
        });
        Ok(link)
    }
    /// Supply a distinct cookie-free transport; no member session is copied.
    pub fn with_anonymous_rooms<R: Transport>(self, transport: R) -> Link<T, AnonymousRooms<R>> {
        Link {
            client: self.client,
            rooms: AnonymousRooms {
                client: cfrm::Client::new(transport, MAX_RESPONSE_BYTES),
                state: rooms::State::default(),
            },
            scope: self.scope,
            attendance: self.attendance,
        }
    }
}
impl<T: Transport, R> Link<T, R> {
    /// Read current published policy through the sole generated Forum client.
    /// Runtime verifies the original signed publication with Charter and applies
    /// the original Rulebook version comparison before starting member work.
    pub async fn settings(&self) -> Result<cfrm::SettingsOutput, ErrorCode> {
        let reply = self
            .client
            .call(cfrm::Request::Settings(cfrm::SettingsInput {}))
            .await?;
        match reply.response {
            cfrm::Output::Settings(settings) => Ok(settings),
            _ => Err(ErrorCode::Transport),
        }
    }
    /// Original configured clock, checked for regression; not a policy grant.
    pub fn now_ms(&self) -> Result<u64, ErrorCode> {
        self.scope.as_ref().ok_or(ErrorCode::InvalidRequest)?.now()
    }
    pub fn community(&self) -> Option<&str> {
        self.scope.as_ref().map(|scope| scope.community.as_str())
    }
    pub fn state(&self) -> State {
        let mut attendance = self.attendance.borrow_mut();
        let deadline = attendance
            .session
            .as_ref()
            .map(|session| session.expires_at_ms)
            .or_else(|| attendance.pending.as_ref().map(|pending| pending.deadline));
        if let (Some(scope), Some(deadline)) = (&self.scope, deadline) {
            match scope.now() {
                Ok(now) if now < deadline => {}
                Ok(_) => attendance.clear(State::Disconnected),
                Err(_) => attendance.clear(State::Unavailable),
            }
        }
        attendance.state
    }
    pub async fn connect(&mut self, input: ChallengeInput) -> Result<Challenge, ErrorCode> {
        self.disconnect();
        if input.publication.is_empty() {
            return Err(ErrorCode::InvalidRequest);
        }
        if input.publication.len() > MAX_CREDENTIAL_BYTES {
            return Err(ErrorCode::TooLarge);
        }
        self.attendance.borrow_mut().state = State::Connecting;
        let mut attempt = Attempt {
            attendance: &self.attendance,
            finished: false,
        };
        let publication = input.publication.clone();
        let result = self.client.call(cfrm::Request::Challenge(input)).await;
        let reply = attempt.result(result)?;
        let cfrm::Output::Challenge(output) = reply.response else {
            return Err(ErrorCode::Transport);
        };
        let scope = self.scope.as_ref().ok_or(ErrorCode::InvalidRequest)?;
        let challenge: cswb::Challenge =
            serde_json::from_slice(&output.challenge).map_err(|_| ErrorCode::Transport)?;
        if scope.now().map_err(|_| ErrorCode::Transport)? >= challenge.expires_at_ms {
            return Err(ErrorCode::ResetRequired);
        }
        let message =
            cswb::challenge_message(&scope.origin, &scope.community, &challenge, &publication);
        self.attendance.borrow_mut().pending = Some(Pending {
            wire: output.challenge,
            publication,
            deadline: challenge.expires_at_ms,
        });
        attempt.finished = true;
        Ok(Challenge { message })
    }
    pub async fn enter(&mut self, signature: Vec<u8>) -> Result<(), ErrorCode> {
        if self.state() != State::Connecting {
            return Err(ErrorCode::ResetRequired);
        }
        let pending = self
            .attendance
            .borrow_mut()
            .pending
            .take()
            .ok_or(ErrorCode::ResetRequired)?;
        let mut attempt = Attempt {
            attendance: &self.attendance,
            finished: false,
        };
        let publication = pending.publication.clone();
        let reply = attempt.result(
            self.client
                .call(cfrm::Request::Enter(cfrm::EnterInput {
                    challenge: pending.wire,
                    signature,
                    publication: pending.publication,
                    record_proof: vec![],
                }))
                .await,
        )?;
        let cfrm::Output::Enter(session) = reply.response else {
            return Err(ErrorCode::Transport);
        };
        let scope = self.scope.as_ref().ok_or(ErrorCode::InvalidRequest)?;
        if session.revision.len() != 16
            || session.session.is_empty()
            || scope.now().map_err(|_| ErrorCode::Transport)? >= session.expires_at_ms
        {
            return Err(ErrorCode::Transport);
        }
        let mut attendance = self.attendance.borrow_mut();
        attendance.session = Some(session);
        attendance.publication = Some(publication);
        attendance.state = State::Present;
        attempt.finished = true;
        Ok(())
    }
    /// Fill the exact live session/nonce and current storage revision in an
    /// owner request, then return bytes for the actual device signing owner.
    pub fn prepare(
        &self,
        mut request: cfrm::Request,
        nonce: [u8; 16],
    ) -> Result<Prepared, ErrorCode> {
        if self.state() != State::Present || nonce == [0; 16] {
            return Err(ErrorCode::ResetRequired);
        }
        let scope = self.scope.as_ref().ok_or(ErrorCode::InvalidRequest)?;
        let attendance = self.attendance.borrow();
        let session = attendance
            .session
            .as_ref()
            .ok_or(ErrorCode::ResetRequired)?;
        match &mut request {
            cfrm::Request::Heartbeat(input) => input.expected_revision = session.revision.clone(),
            cfrm::Request::Replace(input) => input.expected_revision = session.revision.clone(),
            _ => {}
        }
        let proof = request.possession_mut().ok_or(ErrorCode::InvalidRequest)?;
        proof.session = session.session.clone();
        proof.request_nonce = nonce.to_vec();
        proof.signature.clear();
        let decoded = serde_json::from_slice(&session.session).map_err(|_| ErrorCode::Transport)?;
        let message = cswb::request_message(
            &scope.origin,
            request.info().name,
            &decoded,
            &nonce,
            &request.unsigned_bytes()?,
        );
        Ok(Prepared {
            request,
            message,
            session: session.session.clone(),
        })
    }
    pub async fn execute(
        &mut self,
        mut prepared: Prepared,
        signature: Vec<u8>,
    ) -> Result<cfrm::Reply, ErrorCode> {
        if self.state() != State::Present {
            return Err(ErrorCode::ResetRequired);
        }
        if self
            .attendance
            .borrow()
            .session
            .as_ref()
            .is_none_or(|session| session.session != prepared.session)
        {
            return Err(ErrorCode::ResetRequired);
        }
        prepared
            .request
            .possession_mut()
            .ok_or(ErrorCode::InvalidRequest)?
            .signature = signature;
        let mut attempt = Attempt {
            attendance: &self.attendance,
            finished: false,
        };
        let replacement = match &prepared.request {
            cfrm::Request::Replace(input) => Some(input.publication.clone()),
            _ => None,
        };
        let reply = attempt.result(self.client.call(prepared.request).await)?;
        let mut attendance = self.attendance.borrow_mut();
        match &reply.response {
            cfrm::Output::Heartbeat(live) | cfrm::Output::Replace(live) => {
                if live.revision.len() != 16 {
                    return Err(ErrorCode::Transport);
                }
                let session = attendance
                    .session
                    .as_mut()
                    .ok_or(ErrorCode::ResetRequired)?;
                session.revision = live.revision.clone();
                session.expires_at_ms = live.expires_at_ms;
            }
            cfrm::Output::Depart(_) => attendance.clear(State::Disconnected),
            _ => {}
        }
        if let Some(publication) = replacement {
            attendance.publication = Some(publication);
        }
        drop(attendance);
        attempt.finished = true;
        // A late success cannot make an expired page or lease appear live.
        let state = self.state();
        if !matches!(reply.response, cfrm::Output::Depart(_)) && state != State::Present {
            return Err(ErrorCode::ResetRequired);
        }
        Ok(reply)
    }
    pub fn publication(&self) -> Option<Vec<u8>> {
        if self.state() != State::Present {
            return None;
        }
        self.attendance.borrow().publication.clone()
    }
    pub fn disconnect(&mut self) {
        self.attendance.borrow_mut().clear(State::Disconnected);
    }
}
impl<T: Transport, R: Transport> Link<T, AnonymousRooms<R>> {
    pub async fn room_order(&self, input: RoomInput) -> Result<cfrm::Reply, ErrorCode> {
        self.room(input, cfrm::Request::RoomOrder).await
    }
    pub async fn room_relay(&self, input: RoomInput) -> Result<cfrm::Reply, ErrorCode> {
        self.room(input, cfrm::Request::RoomRelay).await
    }
    pub async fn room_resume(&self, input: RoomInput) -> Result<cfrm::Reply, ErrorCode> {
        self.room(input, cfrm::Request::RoomResume).await
    }
    pub async fn room_handover(&self, input: RoomInput) -> Result<cfrm::Reply, ErrorCode> {
        self.room(input, cfrm::Request::RoomHandover).await
    }
    async fn room(
        &self,
        input: RoomInput,
        request: fn(RoomInput) -> cfrm::Request,
    ) -> Result<cfrm::Reply, ErrorCode> {
        if input.room_pass.len().saturating_add(input.command.len()) > MAX_ROOM_INPUT_BYTES {
            return Err(ErrorCode::TooLarge);
        }
        self.rooms.client.call(request(input)).await
    }
}
struct Attempt<'a> {
    attendance: &'a RefCell<Attendance>,
    finished: bool,
}
impl Attempt<'_> {
    fn result(&mut self, result: Result<cfrm::Reply, ErrorCode>) -> Result<cfrm::Reply, ErrorCode> {
        if let Err(error) = result {
            self.attendance
                .borrow_mut()
                .clear(if error == ErrorCode::AuthorityUnavailable {
                    State::Unavailable
                } else {
                    State::Disconnected
                });
            self.finished = true;
            Err(error)
        } else {
            result
        }
    }
}
impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.attendance.borrow_mut().clear(State::Disconnected);
        }
    }
}
