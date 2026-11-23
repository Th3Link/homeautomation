//! Run with tests/chrony.sh, which provides a local NTS server and its CA.
#![cfg(any(feature = "tokio", feature = "async-net"))]

use nts_client::client::RootCertStore;
use std::{env, fs};

fn port() -> u16 {
    env::var("NTS_TEST_KE_PORT").unwrap().parse().unwrap()
}

fn roots() -> RootCertStore {
    let mut store = RootCertStore::empty();
    store
        .add(fs::read(env::var("NTS_TEST_CA").unwrap()).unwrap().into())
        .unwrap();
    store
}

macro_rules! check_peer {
    ($backend:path) => {{
        use $backend as backend;
        assert!(
            backend::key_exchange("127.0.0.1", port(), RootCertStore::empty())
                .await
                .is_err()
        );
        let mut peer = backend::key_exchange("127.0.0.1", port(), roots())
            .await
            .unwrap();
        assert_eq!(
            peer.endpoint().1,
            env::var("NTS_TEST_UDP_PORT")
                .unwrap()
                .parse::<u16>()
                .unwrap()
        );
        for _ in 0..2 {
            let time = backend::query(&mut peer).await.unwrap();
            assert!(time.transmit > 0 && (1..16).contains(&time.stratum));
            assert!(time.cookies > 0 && peer.cookies() > 0);
        }
    }};
}

#[cfg(feature = "tokio")]
#[test]
#[ignore = "requires tests/chrony.sh"]
fn tokio_client() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async { check_peer!(nts_client::client::tokio) });
}

#[cfg(feature = "async-net")]
#[test]
#[ignore = "requires tests/chrony.sh"]
fn smol_client() {
    smol::block_on(async { check_peer!(nts_client::client::async_net) });
}
