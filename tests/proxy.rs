//! The proxy of `[network]` reaches the clients this app does not build.
//!
//! sonora's providers build their own reqwest clients, so the proxy is
//! handed over in the environment, and a SOCKS one is only spoken to by a
//! reqwest built with its `socks` feature: without it the request fails
//! before it connects. Both reqwests in the tree are checked: 0.13 carries
//! the providers and the covers, 0.12 YouTube Music.
//!
//! One test, because it changes the environment of the whole process.

use std::io::Read;
use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use sonora_tui::config::NetworkConfig;

/// The first byte of a SOCKS5 greeting: the protocol version.
const SOCKS5: u8 = 5;

/// A listener, and the first byte the next connection sends it.
fn listener() -> (std::net::SocketAddr, Receiver<u8>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, first) = channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        let _ = sender.send(byte[0]);
    });
    (address, first)
}

/// What a listener heard first, or nothing when no one spoke to it.
fn heard(first: Receiver<u8>) -> Option<u8> {
    first.recv_timeout(Duration::from_secs(5)).ok()
}

#[tokio::test]
async fn clients_built_without_a_proxy_go_through_the_one_the_config_names() {
    let (for_the_providers, providers) = listener();
    // Typed in capitals, which reqwest would skip if handed on as typed.
    let network = |proxy: std::net::SocketAddr| NetworkConfig {
        proxy: format!("SOCKS5H://{proxy}"),
    };
    network(for_the_providers).name_in_environment().unwrap();

    // The stand-in never answers, so the request itself fails either way.
    let _ = reqwest::Client::new()
        .get("http://service.invalid/")
        .send()
        .await;
    assert_eq!(heard(providers), Some(SOCKS5), "reqwest 0.13");

    let (for_youtube_music, youtube_music) = listener();
    network(for_youtube_music).name_in_environment().unwrap();
    let _ = reqwest_012::Client::new()
        .get("http://service.invalid/")
        .send()
        .await;
    assert_eq!(heard(youtube_music), Some(SOCKS5), "reqwest 0.12");

    // A server on this machine is asked directly: the proxy hears nothing,
    // and the server gets the request line itself.
    let (unused, proxy) = listener();
    let (server, asked) = listener();
    network(unused).name_in_environment().unwrap();
    let _ = reqwest::Client::new()
        .get(format!("http://{server}/"))
        .send()
        .await;
    assert_eq!(heard(asked), Some(b'G'), "the server hears GET");
    assert!(proxy.try_recv().is_err(), "and the proxy nothing");
}
