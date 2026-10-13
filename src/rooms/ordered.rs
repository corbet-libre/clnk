//! Actual anonymous transport acknowledgement, never MLS membership authority.
use super::*;

/// One exact room Order acknowledged through this Link's existing anonymous
/// connection. No decoder, clone or public constructor can turn wire data into
/// this value. Its borrow retains the same pass/common authority until the
/// member-side checkpoint consumes the acknowledgement.
///
/// The server orders opaque bytes. This says nothing about their MLS validity,
/// roster, consent, winner or room membership: Threads must verify those through
/// the genuine corresponding candidate before its shared checkpoint commits.
pub struct OrderedRoom<'a> {
    scope: &'a Scope,
    common: &'a Common,
    connection: cmbl::Id,
    command: cmbl::Request,
    outcome: cmbl::Outcome,
    checked_at: u64,
}
impl OrderedRoom<'_> {
    pub fn command(&self) -> &cmbl::Request {
        &self.command
    }
    pub fn ordering(&self) -> cmbl::Id {
        self.outcome.ordering
    }
    pub fn counter(&self) -> u64 {
        self.outcome.counter
    }
    pub fn connection(&self) -> cmbl::Id {
        self.connection
    }
    pub fn epoch(&self) -> &Epoch {
        &self.common.epoch
    }
    pub fn incarnation(&self) -> &str {
        &self.common.incarnation
    }
    /// Re-sample Link's actual retained clock, including its observation floor.
    /// The caller cannot supply a replacement timestamp or renew the pass.
    pub fn check_current(&self) -> Result<u64, ErrorCode> {
        let now = self.scope.now()?;
        if now < self.checked_at
            || now < self.common.epoch.valid_from
            || now >= self.common.epoch.valid_until
        {
            return Err(ErrorCode::ResetRequired);
        }
        Ok(now)
    }
}

impl<T: Transport, R: Transport> Link<T, AnonymousRooms<R>> {
    /// Order the already prepared opaque commit on the existing anonymous
    /// transport. `ordering` is the independently retained transport generation;
    /// a reset never authorizes replay of a previous MLS transition.
    pub async fn order_room(
        &self,
        command: cmbl::Request,
        ordering: cmbl::Id,
    ) -> Result<OrderedRoom<'_>, ErrorCode> {
        let cmbl::Action::Order {
            expected,
            ciphertext,
        } = &command.action
        else {
            return Err(ErrorCode::InvalidRequest);
        };
        let next = expected.checked_add(1).ok_or(ErrorCode::InvalidRequest)?;
        if ordering == [0; 16] || ciphertext.is_empty() {
            return Err(ErrorCode::InvalidRequest);
        }
        let scope = self.scope.as_ref().ok_or(ErrorCode::InvalidRequest)?;
        let common = self
            .rooms
            .state
            .common
            .as_ref()
            .ok_or(ErrorCode::ResetRequired)?;
        let connection = self
            .rooms
            .state
            .passes
            .get(&command.room)
            .ok_or(ErrorCode::ResetRequired)?
            .connection();
        // This executes real pass possession and the exact typed RoomOrder
        // transport response, with original currentness checks around the await.
        let outcome = self.execute_room(command.clone()).await?;
        if outcome.ordering != ordering || outcome.counter != next {
            return Err(ErrorCode::Reconcile);
        }
        let result = OrderedRoom {
            scope,
            common,
            connection,
            command,
            outcome,
            checked_at: scope.now()?,
        };
        result.check_current()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
