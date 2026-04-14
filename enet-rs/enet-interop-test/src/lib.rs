//! Interoperability tests between the Rust `enet-rs` implementation and the
//! original C ENet library.
//!
//! Two scenarios are verified:
//!   1. **Rust server ↔ C client** — the Rust server echoes; the C client
//!      sends a packet and checks the reply.
//!   2. **C server ↔ Rust client** — the C server echoes; the Rust client
//!      sends a packet and checks the reply.

#[cfg(test)]
mod interop_tests {
    use std::ffi::{c_int, CString};
    use std::time::Duration;

    use enet_rs::{Host, HostConfig, Packet};

    // ---------------------------------------------------------------------------
    // FFI — functions implemented in c_helper.c / the compiled C ENet library
    // ---------------------------------------------------------------------------

    extern "C" {
        /// Bind an ENet server on `port`, accept one client, echo one packet, exit.
        fn c_server_echo_once(port: u16) -> c_int;

        /// Connect to `host_str:port`, send `data[..data_len]`, receive the reply.
        /// Copies at most `out_buf_len` bytes into `out_buf`.
        /// Returns the number of bytes received, or -1 on failure.
        fn c_client_send_recv(
            host_str: *const std::ffi::c_char,
            port: u16,
            data: *const u8,
            data_len: usize,
            out_buf: *mut u8,
            out_buf_len: usize,
        ) -> c_int;
    }

    // ---------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------

    /// Reserve a random free local UDP port.
    fn free_port() -> u16 {
        std::net::UdpSocket::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    // ---------------------------------------------------------------------------
    // Test 1: Rust server  ←→  C client
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_rust_server_c_client() {
        let port = free_port();
        let addr = format!("127.0.0.1:{port}");

        // Bind the server BEFORE spawning the C client.
        // This entirely eliminates the need to sleep while waiting for the server to spin up.
        let mut server = Host::bind(&addr, HostConfig::default()).await.unwrap();

        let msg: &[u8] = b"hello from C client";
        let msg_vec = msg.to_vec();

        // Run the C client on a dedicated OS thread (C ENet is synchronous).
        let c_thread = std::thread::spawn(move || {
            let host_cstr = CString::new("127.0.0.1").unwrap();
            let mut out_buf = vec![0u8; 1024];
            let n = unsafe {
                c_client_send_recv(
                    host_cstr.as_ptr(),
                    port,
                    msg_vec.as_ptr(),
                    msg_vec.len(),
                    out_buf.as_mut_ptr(),
                    out_buf.len(),
                )
            };
            assert!(n >= 0, "C client failed (returned {n})");
            out_buf[..n as usize].to_vec()
        });

        // Accept one peer, receive one packet, echo it back.
        let mut peer = tokio::time::timeout(Duration::from_secs(2), server.accept())
            .await
            .expect("Server accept timed out")
            .expect("Server closed unexpectedly");

        let pkt = tokio::time::timeout(Duration::from_secs(2), peer.recv())
            .await
            .expect("Server recv timed out")
            .expect("Server recv failed");

        let reply = Packet::reliable(pkt.data.clone(), pkt.channel);
        peer.send_packet(reply).await.expect("Failed to send reply");

        // Wait for the C client to disconnect instead of an arbitrary 500ms sleep.
        // This naturally forces `enet-rs` to flush the packet but finishes INSTANTLY
        // the moment the C client closes the connection.
        let _ = tokio::time::timeout(Duration::from_secs(2), peer.recv()).await;

        let received = c_thread.join().expect("C client thread panicked");
        assert_eq!(
            received.as_slice(),
            msg,
            "Test 1: echo mismatch — got {:?}",
            received
        );
    }

    // ---------------------------------------------------------------------------
    // Test 2: C server  ←→  Rust client
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_c_server_rust_client() {
        let port = free_port();

        // Spawn the C server on an OS thread (blocking, synchronous).
        let c_server = std::thread::spawn(move || {
            let rc = unsafe { c_server_echo_once(port) };
            assert_eq!(rc, 0, "C server returned error code {rc}");
        });

        // Give the C server a tiny moment to bind. Reduced from 100ms to 10ms.
        // ENet connects are reliable and will retry automatically, so a long sleep isn't needed.
        tokio::time::sleep(Duration::from_millis(10)).await;

        let msg: &[u8] = b"hello from Rust client";
        let addr = format!("127.0.0.1:{port}");

        let mut peer = Host::connect(&addr, 2)
            .await
            .expect("Rust client connect failed");

        peer.send_packet(Packet::reliable(msg, 0))
            .await
            .expect("send failed");

        let reply = tokio::time::timeout(Duration::from_secs(2), peer.recv())
            .await
            .expect("timeout waiting for echo")
            .expect("peer disconnected before reply");

        assert_eq!(
            reply.data.as_ref(),
            msg,
            "Test 2: echo mismatch — got {:?}",
            reply.data
        );

        c_server.join().expect("C server thread panicked");
    }
}
