//! Private original pass capabilities and their two distinct connection classes.
use super::*;
mod ordered;
use cmbl::passes::{ClientPass, Epoch, PreparedPass};
pub use ordered::OrderedRoom;
use std::collections::BTreeMap;

const MAX_ROOM_CAPABILITIES: usize = 32;
struct Common {
    epoch: Epoch,
    incarnation: String,
}
struct Pending {
    room: cmbl::Id,
    pass: PreparedPass,
}
#[derive(Default)]
pub(super) struct State {
    common: Option<Common>,
    pending: BTreeMap<cmbl::Id, Pending>,
    passes: BTreeMap<cmbl::Id, ClientPass>,
}
fn empty_possession() -> cfrm::Possession {
    cfrm::Possession {
        session: vec![],
        request_nonce: vec![],
        signature: vec![],
    }
}
impl<T: Transport, R: Transport> Link<T, AnonymousRooms<R>> {
    /// The common descriptor is received only through authenticated Forum.
    pub fn prepare_room_epoch(&self, nonce: cmbl::Id) -> Result<Prepared, ErrorCode> {
        self.prepare(
            cfrm::Request::RoomEpoch(cfrm::RoomEpochInput {
                possession: empty_possession(),
            }),
            nonce,
        )
    }
    pub async fn receive_room_epoch(
        &mut self,
        prepared: Prepared,
        signature: Vec<u8>,
    ) -> Result<(), ErrorCode> {
        if !matches!(prepared.request, cfrm::Request::RoomEpoch(_)) {
            return Err(ErrorCode::InvalidRequest);
        }
        let reply = self.execute(prepared, signature).await?;
        let cfrm::Output::RoomEpoch(output) = reply.response else {
            return Err(ErrorCode::Transport);
        };
        let epoch: Epoch =
            serde_json::from_slice(&output.epoch).map_err(|_| ErrorCode::Transport)?;
        let scope = self.scope.as_ref().ok_or(ErrorCode::InvalidRequest)?;
        let now = scope.now()?;
        if epoch.community != scope.community || now < epoch.valid_from || now >= epoch.valid_until
        {
            return Err(ErrorCode::Transport);
        }
        epoch.context().map_err(|_| ErrorCode::Transport)?;
        cmmr::validate_name(&output.incarnation).map_err(|_| ErrorCode::Transport)?;
        let state = &mut self.rooms.state;
        if state
            .common
            .as_ref()
            .is_none_or(|old| old.epoch != epoch || old.incarnation != output.incarnation)
        {
            // Replaced common authority invalidates every old anonymous key.
            state.pending.clear();
            state.passes.clear();
        }
        state.common = Some(Common {
            epoch,
            incarnation: output.incarnation,
        });
        Ok(())
    }
    /// Retain the original blinding state. An exact operation retry uses those
    /// same bytes; a changed room or lost state never resumes that operation.
    pub fn prepare_room_pass(
        &mut self,
        room: cmbl::Id,
        operation: cmbl::Id,
        nonce: cmbl::Id,
    ) -> Result<Prepared, ErrorCode> {
        if operation == [0; 16] {
            return Err(ErrorCode::InvalidRequest);
        }
        let now = self
            .scope
            .as_ref()
            .ok_or(ErrorCode::InvalidRequest)?
            .now()?;
        let state = &mut self.rooms.state;
        let common = state.common.as_ref().ok_or(ErrorCode::ResetRequired)?;
        if now < common.epoch.valid_from || now >= common.epoch.valid_until {
            return Err(ErrorCode::ResetRequired);
        }
        if let Some(pending) = state.pending.get(&operation) {
            if pending.room != room {
                return Err(ErrorCode::InvalidRequest);
            }
        } else {
            if state.passes.contains_key(&room)
                || state.pending.values().any(|pending| pending.room == room)
            {
                return Err(ErrorCode::Reconcile);
            }
            if state.passes.len().saturating_add(state.pending.len()) >= MAX_ROOM_CAPABILITIES {
                return Err(ErrorCode::Capacity);
            }
            let pass =
                PreparedPass::new(&common.epoch, room, now).map_err(|_| ErrorCode::Rejected)?;
            state.pending.insert(operation, Pending { room, pass });
        }
        let pending = state.pending.get(&operation).ok_or(ErrorCode::Reconcile)?;
        let blinded_request =
            serde_json::to_vec(&pending.pass.request()).map_err(|_| ErrorCode::InvalidRequest)?;
        self.prepare(
            cfrm::Request::RoomIssue(cfrm::RoomIssueInput {
                possession: empty_possession(),
                operation: operation.to_vec(),
                blinded_request,
            }),
            nonce,
        )
    }
    /// Only an authenticated original issuance response can finalize the private
    /// original pass. Interrupted remote work retains the exact pending request.
    pub async fn receive_room_pass(
        &mut self,
        prepared: Prepared,
        signature: Vec<u8>,
    ) -> Result<cmbl::Id, ErrorCode> {
        let cfrm::Request::RoomIssue(input) = &prepared.request else {
            return Err(ErrorCode::InvalidRequest);
        };
        let operation: cmbl::Id = input
            .operation
            .as_slice()
            .try_into()
            .map_err(|_| ErrorCode::InvalidRequest)?;
        let pending = self
            .rooms
            .state
            .pending
            .get(&operation)
            .ok_or(ErrorCode::Reconcile)?;
        if serde_json::to_vec(&pending.pass.request()).map_err(|_| ErrorCode::InvalidRequest)?
            != input.blinded_request
        {
            return Err(ErrorCode::Reconcile);
        }
        let reply = self.execute(prepared, signature).await?;
        let cfrm::Output::RoomIssue(output) = reply.response else {
            return Err(ErrorCode::Transport);
        };
        let now = self
            .scope
            .as_ref()
            .ok_or(ErrorCode::InvalidRequest)?
            .now()?;
        let pending = self
            .rooms
            .state
            .pending
            .remove(&operation)
            .ok_or(ErrorCode::Reconcile)?;
        let pass = pending
            .pass
            .finalize(&output.signature, now)
            .map_err(|_| ErrorCode::Rejected)?;
        let connection = pass.connection();
        self.rooms.state.passes.insert(pending.room, pass);
        Ok(connection)
    }
    pub fn room_connection(&self, room: cmbl::Id) -> Result<cmbl::Id, ErrorCode> {
        let now = self
            .scope
            .as_ref()
            .ok_or(ErrorCode::InvalidRequest)?
            .now()?;
        let common = self
            .rooms
            .state
            .common
            .as_ref()
            .ok_or(ErrorCode::ResetRequired)?;
        if now < common.epoch.valid_from || now >= common.epoch.valid_until {
            return Err(ErrorCode::ResetRequired);
        }
        self.rooms
            .state
            .passes
            .get(&room)
            .map(ClientPass::connection)
            .ok_or(ErrorCode::ResetRequired)
    }
    /// Anonymous transport receives only Assembly's original command and proof;
    /// the presence session and device request signer are never copied there.
    pub async fn execute_room(&self, command: cmbl::Request) -> Result<cmbl::Outcome, ErrorCode> {
        let scope = self.scope.as_ref().ok_or(ErrorCode::InvalidRequest)?;
        let now = scope.now()?;
        let state = &self.rooms.state;
        let common = state.common.as_ref().ok_or(ErrorCode::ResetRequired)?;
        let pass = state
            .passes
            .get(&command.room)
            .ok_or(ErrorCode::ResetRequired)?;
        let proof = pass
            .prove(&command, &common.incarnation, now)
            .map_err(|_| ErrorCode::Rejected)?;
        let input = RoomInput {
            room_pass: proof,
            command: serde_json::to_vec(&command).map_err(|_| ErrorCode::InvalidRequest)?,
        };
        if input.room_pass.len().saturating_add(input.command.len()) > MAX_ROOM_INPUT_BYTES {
            return Err(ErrorCode::TooLarge);
        }
        let request = match &command.action {
            cmbl::Action::Connect { .. } => cfrm::Request::RoomConnect(input),
            cmbl::Action::Renew { .. } => cfrm::Request::RoomRenew(input),
            cmbl::Action::Disconnect { .. } => cfrm::Request::RoomDisconnect(input),
            cmbl::Action::Read { .. } => cfrm::Request::RoomRead(input),
            cmbl::Action::Order { .. } => cfrm::Request::RoomOrder(input),
            cmbl::Action::Relay { .. } => cfrm::Request::RoomRelay(input),
            cmbl::Action::Resume { .. } => cfrm::Request::RoomResume(input),
            // The forum's room_publish request is not on cfrm v01 yet.
            cmbl::Action::Publish { .. } => return Err(ErrorCode::InvalidRequest),
        };
        let reply = self.rooms.client.call(request).await?;
        if scope.now()? >= common.epoch.valid_until {
            return Err(ErrorCode::ResetRequired);
        }
        let result = match (&command.action, reply.response) {
            (cmbl::Action::Connect { .. }, cfrm::Output::RoomConnect(v))
            | (cmbl::Action::Renew { .. }, cfrm::Output::RoomRenew(v))
            | (cmbl::Action::Disconnect { .. }, cfrm::Output::RoomDisconnect(v))
            | (cmbl::Action::Read { .. }, cfrm::Output::RoomRead(v))
            | (cmbl::Action::Order { .. }, cfrm::Output::RoomOrder(v))
            | (cmbl::Action::Relay { .. }, cfrm::Output::RoomRelay(v))
            | (cmbl::Action::Resume { .. }, cfrm::Output::RoomResume(v)) => v.result,
            _ => return Err(ErrorCode::Transport),
        };
        serde_json::from_slice(&result).map_err(|_| ErrorCode::Transport)
    }
    /// Explicit room lifetime termination drops the opaque connection signers.
    pub fn close_rooms(&mut self) {
        self.rooms.state = State::default();
    }
}
