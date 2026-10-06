use clnk::*;
#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
macro_rules! shared {
    ($name:ident,$body:block)=>{
        #[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
        #[cfg_attr(not(target_arch="wasm32"),test)]
        fn $name() $body
    };
}
// Executes the actual Forum registry in-process. This checks generated-client
// plumbing and refusal only; it is explicitly not HTTPS or admission evidence.
struct Owner;
impl Transport for Owner {
    async fn post(&self, path: &str, body: Vec<u8>) -> Result<Vec<u8>, ErrorCode> {
        let call = cfrm::Call::decode(&body, MAX_CREDENTIAL_BYTES * 4)?;
        assert_eq!(path, format!("/v1/{}", call.request.info().name));
        let error = cfrm::execute(&call).unwrap_err();
        Ok(serde_json::to_vec(&cfrm::Failure { error }).unwrap())
    }
}
struct Pending;
impl Transport for Pending {
    async fn post(&self, _: &str, _: Vec<u8>) -> Result<Vec<u8>, ErrorCode> {
        std::future::pending().await
    }
}
struct Offline;
impl Transport for Offline {
    async fn post(&self, _: &str, _: Vec<u8>) -> Result<Vec<u8>, ErrorCode> {
        Err(ErrorCode::Transport)
    }
}
shared!(actual_forum_refusal_never_creates_presence, {
    let mut link = Link::new(Owner);
    assert_eq!(link.state(), State::Disconnected);
    assert_eq!(
        futures_executor::block_on(link.connect(ChallengeInput {
            publication: vec![1]
        })),
        Err(ErrorCode::AuthorityUnavailable)
    );
    assert_eq!(link.state(), State::Unavailable);
    link.disconnect();
    assert_eq!(link.state(), State::Disconnected);
    for bytes in [vec![], vec![0; MAX_CREDENTIAL_BYTES + 1]] {
        assert!(
            futures_executor::block_on(link.connect(ChallengeInput { publication: bytes }))
                .is_err()
        );
        assert_eq!(link.state(), State::Disconnected);
    }
    let mut offline = Link::new(Offline);
    assert_eq!(
        futures_executor::block_on(offline.connect(ChallengeInput {
            publication: vec![1]
        })),
        Err(ErrorCode::Transport)
    );
    assert_eq!(offline.state(), State::Disconnected);
});
shared!(cancelled_connect_has_no_stale_presence, {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let mut link = Link::new(Pending);
    let mut attempt = Box::pin(link.connect(ChallengeInput {
        publication: vec![1],
    }));
    assert!(matches!(
        attempt
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    drop(attempt);
    assert_eq!(link.state(), State::Disconnected);
});

struct RecordingOwner {
    expected: cfrm::Requirement,
    calls: std::rc::Rc<std::cell::RefCell<Vec<cfrm::Call>>>,
}
impl Transport for RecordingOwner {
    async fn post(&self, path: &str, body: Vec<u8>) -> Result<Vec<u8>, ErrorCode> {
        let call = cfrm::Call::decode(&body, MAX_ROOM_INPUT_BYTES * 5)?;
        assert_eq!(call.request.info().requirement, self.expected);
        assert_eq!(path, format!("/v1/{}", call.request.info().name));
        let error = cfrm::execute(&call).unwrap_err();
        self.calls.borrow_mut().push(call);
        Ok(serde_json::to_vec(&cfrm::Failure { error }).unwrap())
    }
}

shared!(room_requests_use_only_the_separate_owner_client, {
    use std::{cell::RefCell, rc::Rc};
    let authenticated = Rc::new(RefCell::new(Vec::new()));
    let anonymous = Rc::new(RefCell::new(Vec::new()));
    let mut link = Link::new(RecordingOwner {
        expected: cfrm::Requirement::Credential,
        calls: authenticated.clone(),
    });
    assert_eq!(
        futures_executor::block_on(link.connect(ChallengeInput {
            publication: vec![7]
        })),
        Err(ErrorCode::AuthorityUnavailable)
    );
    let mut link = link.with_anonymous_rooms(RecordingOwner {
        expected: cfrm::Requirement::AnonymousRoomPass,
        calls: anonymous.clone(),
    });
    assert_eq!(link.state(), State::Unavailable);
    let input = RoomInput {
        room_pass: vec![8],
        command: vec![9],
    };
    assert_eq!(
        futures_executor::block_on(link.room_order(input.clone())),
        Err(ErrorCode::AuthorityUnavailable)
    );
    assert_eq!(
        futures_executor::block_on(link.room_relay(input.clone())),
        Err(ErrorCode::AuthorityUnavailable)
    );
    assert_eq!(
        futures_executor::block_on(link.room_resume(input.clone())),
        Err(ErrorCode::AuthorityUnavailable)
    );
    assert_eq!(
        futures_executor::block_on(link.room_handover(input)),
        Err(ErrorCode::AuthorityUnavailable)
    );
    assert_eq!(link.state(), State::Unavailable);
    assert_eq!(authenticated.borrow().len(), 1);
    assert_eq!(anonymous.borrow().len(), 4);
    for call in anonymous.borrow().iter() {
        let wire = serde_json::to_value(call).unwrap();
        let input = wire["request"]["input"].as_object().unwrap();
        assert_eq!(input.len(), 2);
        assert_eq!(input["room_pass"], serde_json::json!([8]));
        assert_eq!(input["command"], serde_json::json!([9]));
    }
    // The exact combined boundary reaches the real owner; excess never reaches
    // either transport and cannot change authenticated state.
    assert_eq!(
        futures_executor::block_on(link.room_order(RoomInput {
            room_pass: vec![1],
            command: vec![0; MAX_ROOM_INPUT_BYTES - 1],
        })),
        Err(ErrorCode::AuthorityUnavailable)
    );
    assert_eq!(
        futures_executor::block_on(link.room_order(RoomInput {
            room_pass: vec![1],
            command: vec![0; MAX_ROOM_INPUT_BYTES],
        })),
        Err(ErrorCode::TooLarge)
    );
    assert_eq!(anonymous.borrow().len(), 5);
    assert_eq!(
        futures_executor::block_on(link.connect(ChallengeInput {
            publication: vec![6]
        })),
        Err(ErrorCode::AuthorityUnavailable)
    );
    assert_eq!(authenticated.borrow().len(), 2);
    assert_eq!(anonymous.borrow().len(), 5);
    link.disconnect();
    assert_eq!(link.state(), State::Disconnected);
});

shared!(room_combined_input_boundary_holds_for_all_operations, {
    use std::{cell::RefCell, rc::Rc};
    let authenticated = Rc::new(RefCell::new(Vec::new()));
    let anonymous = Rc::new(RefCell::new(Vec::new()));
    let mut link = Link::new(RecordingOwner {
        expected: cfrm::Requirement::Credential,
        calls: authenticated.clone(),
    });
    assert_eq!(
        futures_executor::block_on(link.connect(ChallengeInput {
            publication: vec![7]
        })),
        Err(ErrorCode::AuthorityUnavailable)
    );
    let link = link.with_anonymous_rooms(RecordingOwner {
        expected: cfrm::Requirement::AnonymousRoomPass,
        calls: anonymous.clone(),
    });
    assert_eq!(link.state(), State::Unavailable);
    assert_eq!(authenticated.borrow().len(), 1);
    let mut expected_anonymous = 0;
    for op in ["order", "relay", "resume", "handover"] {
        for input in [
            RoomInput {
                room_pass: vec![1; MAX_ROOM_INPUT_BYTES - 1],
                command: vec![2],
            },
            RoomInput {
                room_pass: vec![1],
                command: vec![2; MAX_ROOM_INPUT_BYTES - 1],
            },
        ] {
            let result = match op {
                "order" => futures_executor::block_on(link.room_order(input)),
                "relay" => futures_executor::block_on(link.room_relay(input)),
                "resume" => futures_executor::block_on(link.room_resume(input)),
                "handover" => futures_executor::block_on(link.room_handover(input)),
                _ => unreachable!(),
            };
            assert_eq!(result, Err(ErrorCode::AuthorityUnavailable));
            expected_anonymous += 1;
            assert_eq!(anonymous.borrow().len(), expected_anonymous);
            assert_eq!(authenticated.borrow().len(), 1);
            assert_eq!(link.state(), State::Unavailable);
        }
        for input in [
            RoomInput {
                room_pass: vec![1; MAX_ROOM_INPUT_BYTES],
                command: vec![2],
            },
            RoomInput {
                room_pass: vec![1],
                command: vec![2; MAX_ROOM_INPUT_BYTES],
            },
        ] {
            let result = match op {
                "order" => futures_executor::block_on(link.room_order(input)),
                "relay" => futures_executor::block_on(link.room_relay(input)),
                "resume" => futures_executor::block_on(link.room_resume(input)),
                "handover" => futures_executor::block_on(link.room_handover(input)),
                _ => unreachable!(),
            };
            assert_eq!(result, Err(ErrorCode::TooLarge));
            assert_eq!(anonymous.borrow().len(), expected_anonymous);
            assert_eq!(authenticated.borrow().len(), 1);
            assert_eq!(link.state(), State::Unavailable);
        }
    }
    assert_eq!(expected_anonymous, 8);
    assert_eq!(anonymous.borrow().len(), 8);
    assert_eq!(authenticated.borrow().len(), 1);
});

shared!(
    anonymous_outage_or_cancellation_never_uses_authenticated_fallback,
    {
        use std::{
            future::Future,
            task::{Context, Poll, Waker},
        };
        let mut link = Link::new(Owner).with_anonymous_rooms(Offline);
        assert_eq!(
            futures_executor::block_on(link.connect(ChallengeInput {
                publication: vec![1]
            })),
            Err(ErrorCode::AuthorityUnavailable)
        );
        assert_eq!(
            futures_executor::block_on(link.room_relay(RoomInput {
                room_pass: vec![1],
                command: vec![2]
            })),
            Err(ErrorCode::Transport)
        );
        assert_eq!(link.state(), State::Unavailable);

        let mut link = Link::new(Owner).with_anonymous_rooms(Pending);
        assert_eq!(
            futures_executor::block_on(link.connect(ChallengeInput {
                publication: vec![1]
            })),
            Err(ErrorCode::AuthorityUnavailable)
        );
        let mut pending = Box::pin(link.room_resume(RoomInput {
            room_pass: vec![1],
            command: vec![2],
        }));
        assert!(matches!(
            pending
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
        drop(pending);
        assert_eq!(link.state(), State::Unavailable);
    }
);

// Link maintained profiling support only into the instrumented test binary.
#[cfg(all(target_arch = "wasm32", owned_browser_coverage))]
#[wasm_bindgen_test::wasm_bindgen_test]
fn profiling_runtime_is_linked() {
    let _ = browser_coverage_runtime::__owned_test_module_signature();
}
