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
            credential: vec![1]
        })),
        Err(ErrorCode::AuthorityUnavailable)
    );
    assert_eq!(link.state(), State::Unavailable);
    link.disconnect();
    assert_eq!(link.state(), State::Disconnected);
    for bytes in [vec![], vec![0; MAX_CREDENTIAL_BYTES + 1]] {
        assert!(
            futures_executor::block_on(link.connect(ChallengeInput { credential: bytes })).is_err()
        );
        assert_eq!(link.state(), State::Disconnected);
    }
    let mut offline = Link::new(Offline);
    assert_eq!(
        futures_executor::block_on(offline.connect(ChallengeInput {
            credential: vec![1]
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
        credential: vec![1],
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

// Link maintained profiling support only into the instrumented test binary.
#[cfg(all(target_arch = "wasm32", owned_browser_coverage))]
#[wasm_bindgen_test::wasm_bindgen_test]
fn profiling_runtime_is_linked() {
    let _ = browser_coverage_runtime::__owned_test_module_signature();
}
