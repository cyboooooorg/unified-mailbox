mod common;

use std::io;
use std::sync::{Arc, Mutex};

use common::{harness, pair, send_auth};
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl io::Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Buffer {
    type Writer = Buffer;

    fn make_writer(&'a self) -> Buffer {
        self.clone()
    }
}

/// Neither the token nor the query string reaches the logs: only the path.
#[tokio::test]
async fn logs_the_path_without_query_or_token() {
    let buffer = Buffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buffer.clone())
        .with_max_level(tracing::Level::INFO)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let res = send_auth(
        &h.router,
        "GET",
        "/api/v1/devices?ticket=secret-value",
        &token,
    )
    .await;
    assert_eq!(res.status(), 200);

    let logs = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("/api/v1/devices"), "{logs}");
    assert!(!logs.contains("secret-value"), "{logs}");
    assert!(!logs.contains("ticket"), "{logs}");
    assert!(!logs.contains(&token), "{logs}");
}
