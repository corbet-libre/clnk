//! Partial transport-owner test: genuine blind pass and Assembly, not G1/G2
//! admission or MLS membership. No accepting membership/proof adapter exists.
use super::*;
use blind_rsa_signatures::{DefaultRng, KeyPair, PSS, Randomized, Sha384};
use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicU64, Ordering},
};

#[derive(Clone)]
struct Time(Arc<AtomicU64>);
impl cmmr::Clock for Time {
    fn now(&self) -> Result<u64, cmmr::Error> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}
struct Peer {
    assembly: cmbl::Assembly<Arc<cmmr::Memory<Time>>>,
    verifier: cmbl::passes::Verifier,
    time: Time,
    mode: Arc<AtomicU8>,
}
impl Transport for Peer {
    async fn post(&self, path: &str, body: Vec<u8>) -> Result<Vec<u8>, ErrorCode> {
        let call = cfrm::Call::decode(&body, MAX_ROOM_INPUT_BYTES * 2)?;
        assert_eq!(path, format!("/v1/{}", call.request.info().name));
        let (input, kind) = match call.request {
            cfrm::Request::RoomResume(input) => (input, 0),
            cfrm::Request::RoomOrder(input) => (input, 1),
            cfrm::Request::RoomRead(input) => (input, 2),
            _ => return Err(ErrorCode::InvalidRequest),
        };
        let command: cmbl::Request = serde_json::from_slice(&input.command).unwrap();
        let result = self
            .assembly
            .execute(
                &command,
                &input.room_pass,
                &self.verifier,
                self.time.now().unwrap(),
            )
            .await;
        let mut result = match result {
            Ok(value) => value,
            Err(_) => {
                return Ok(serde_json::to_vec(&cfrm::Failure {
                    error: ErrorCode::Rejected,
                })
                .unwrap());
            }
        };
        match self.mode.load(Ordering::SeqCst) {
            1 => result.ordering[0] ^= 1,
            2 => result.counter += 1,
            3 => self.time.0.store(500, Ordering::SeqCst),
            5 => std::future::pending::<()>().await,
            _ => (),
        }
        let output = cfrm::RoomOutput {
            result: serde_json::to_vec(&result).unwrap(),
        };
        let response = if kind == 0 {
            cfrm::Output::RoomResume(output)
        } else if kind == 2 {
            cfrm::Output::RoomRead(output)
        } else if self.mode.load(Ordering::SeqCst) == 4 {
            cfrm::Output::RoomRelay(output)
        } else {
            cfrm::Output::RoomOrder(output)
        };
        Ok(serde_json::to_vec(&cfrm::Reply {
            version: 1,
            response,
        })
        .unwrap())
    }
}
struct NoMemberConnection;
impl Transport for NoMemberConnection {
    async fn post(&self, _: &str, _: Vec<u8>) -> Result<Vec<u8>, ErrorCode> {
        panic!("anonymous order must never use the authenticated member transport")
    }
}
fn setup() -> (
    Link<NoMemberConnection, AnonymousRooms<Peer>>,
    Time,
    Arc<AtomicU8>,
) {
    let time = Time(Arc::new(AtomicU64::new(100)));
    let key = KeyPair::<Sha384, PSS, Randomized>::generate(&mut DefaultRng, 3072).unwrap();
    let epoch = Epoch {
        community: "test".into(),
        epoch: 1,
        valid_from: 50,
        valid_until: 500,
        public_key_der: key.pk.to_der().unwrap(),
    };
    let prepared = PreparedPass::new(&epoch, [1; 16], 100).unwrap();
    let signed = key.sk.blind_sign(&prepared.request().blinded).unwrap();
    let pass = prepared.finalize(&signed.0, 100).unwrap();
    let store = Arc::new(
        cmmr::Memory::new(
            cmmr::Scope::new("test", "boot").unwrap(),
            cmmr::Limits {
                records: 8,
                value_bytes: 100_000,
                batch: 8,
                indexes_per_record: 0,
                page: 8,
                ttl_ms: 10_000,
            },
            time.clone(),
        )
        .unwrap(),
    );
    let mode = Arc::new(AtomicU8::new(0));
    let peer = Peer {
        assembly: cmbl::Assembly::new(
            store,
            "test".into(),
            "boot".into(),
            cmbl::Limits {
                connections: 4,
                relay_messages: 8,
                message_bytes: 100,
                lifetime_ms: 1000,
                proof_bytes: 8192,
                operation_receipts: 32,
            },
        )
        .unwrap(),
        verifier: cmbl::passes::Verifier::new(epoch.clone(), "boot".into()).unwrap(),
        time: time.clone(),
        mode: mode.clone(),
    };
    let mut link = Link::at(
        NoMemberConnection,
        "test".into(),
        "https://test.example".into(),
        time.clone(),
    )
    .unwrap()
    .with_anonymous_rooms(peer);
    // This lower owner fixture installs an actually issued/unblinded pass. It
    // deliberately does not claim the missing full Forum admission composition.
    link.rooms.state.common = Some(Common {
        epoch,
        incarnation: "boot".into(),
    });
    link.rooms.state.passes.insert([1; 16], pass);
    (link, time, mode)
}
fn order() -> cmbl::Request {
    cmbl::Request {
        room: [1; 16],
        operation: [3; 16],
        action: cmbl::Action::Order {
            expected: 0,
            ciphertext: vec![7, 8, 9],
        },
    }
}
async fn resume(link: &Link<NoMemberConnection, AnonymousRooms<Peer>>) -> cmbl::Id {
    link.execute_room(cmbl::Request {
        room: [1; 16],
        operation: [2; 16],
        action: cmbl::Action::Resume {
            connection: link.room_connection([1; 16]).unwrap(),
        },
    })
    .await
    .unwrap()
    .ordering
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn actual_pass_and_order_bind_exact_command_retry_generation_and_live_epoch() {
    futures_executor::block_on(async {
        let (link, time, mode) = setup();
        let generation = resume(&link).await;
        let command = order();
        let receipt = link.order_room(command.clone(), generation).await.unwrap();
        assert_eq!(receipt.command(), &command);
        assert_eq!(receipt.counter(), 1);
        assert_eq!(receipt.ordering(), generation);
        assert_eq!(receipt.connection(), link.room_connection([1; 16]).unwrap());
        assert_eq!(receipt.epoch().epoch, 1);
        assert_eq!(receipt.incarnation(), "boot");
        assert_eq!(receipt.check_current(), Ok(100));
        let retry = link.order_room(command.clone(), generation).await.unwrap();
        assert_eq!(retry.counter(), receipt.counter());
        let mut changed = command.clone();
        changed.action = cmbl::Action::Order {
            expected: 0,
            ciphertext: vec![4],
        };
        assert_eq!(
            link.order_room(changed, generation).await.err(),
            Some(ErrorCode::Rejected)
        );
        for (fault, expected) in [
            (1, ErrorCode::Reconcile),
            (2, ErrorCode::Reconcile),
            (4, ErrorCode::Transport),
        ] {
            mode.store(fault, Ordering::SeqCst);
            assert_eq!(
                link.order_room(command.clone(), generation).await.err(),
                Some(expected)
            );
        }
        mode.store(0, Ordering::SeqCst);
        assert_eq!(
            link.order_room(command.clone(), [0; 16]).await.err(),
            Some(ErrorCode::InvalidRequest)
        );
        mode.store(3, Ordering::SeqCst);
        assert_eq!(
            link.order_room(command, generation).await.err(),
            Some(ErrorCode::ResetRequired)
        );
        assert_eq!(receipt.check_current(), Err(ErrorCode::ResetRequired));
        time.0.store(100, Ordering::SeqCst);
        assert_eq!(
            receipt.check_current(),
            Err(ErrorCode::AuthorityUnavailable)
        );
    });
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn cancelled_reply_creates_no_acknowledgement_and_exact_retry_reconciles() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    futures_executor::block_on(async {
        let (link, _, mode) = setup();
        let generation = resume(&link).await;
        mode.store(5, Ordering::SeqCst);
        let mut attempt = Box::pin(link.order_room(order(), generation));
        assert!(matches!(
            attempt
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
        drop(attempt);
        mode.store(0, Ordering::SeqCst);
        let before_retry = link
            .execute_room(cmbl::Request {
                room: [1; 16],
                operation: [4; 16],
                action: cmbl::Action::Read { after: 0 },
            })
            .await
            .unwrap();
        assert_eq!(before_retry.counter, 1);
        assert_eq!(before_retry.frames.len(), 1);
        assert_eq!(before_retry.frames[0].ciphertext, vec![7, 8, 9]);
        // Actual Assembly has already committed; its original operation receipt
        // returns the same order instead of applying the ciphertext twice.
        let receipt = link.order_room(order(), generation).await.unwrap();
        assert_eq!(receipt.counter(), 1);
        let after_retry = link
            .execute_room(cmbl::Request {
                room: [1; 16],
                operation: [5; 16],
                action: cmbl::Action::Read { after: 0 },
            })
            .await
            .unwrap();
        assert_eq!(after_retry.frames, before_retry.frames);
    });
}
