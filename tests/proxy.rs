//! A SOCKS proxy named in `ALL_PROXY` is spoken to as one. reqwest only does
//! so when built with its `socks` feature; without it the proxy is taken for
//! an HTTP one and every request fails. Both reqwests in the tree are checked:
//! 0.13 carries sonora's providers and the covers, 0.12 YouTube Music.

use std::io::Read;
use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

/// The first byte of a SOCKS5 greeting: the protocol version.
const SOCKS5: u8 = 5;

/// A listener standing in for a proxy, and the first byte the next
/// connection sends it.
fn proxy() -> (String, Receiver<u8>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("socks5h://{}", listener.local_addr().unwrap());
    let (sender, first) = channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        let _ = sender.send(byte[0]);
    });
    (address, first)
}

/// What the stand-in heard first, or nothing when no one spoke to it: a
/// reqwest without `socks` gives up before connecting.
fn heard(first: Receiver<u8>) -> Option<u8> {
    first.recv_timeout(Duration::from_secs(5)).ok()
}

#[tokio::test]
async fn the_reqwest_of_the_providers_greets_a_socks_proxy_in_socks() {
    let (address, first) = proxy();
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(address).unwrap())
        .build()
        .unwrap();

    // The stand-in never answers, so the request itself fails either way.
    let _ = client.get("http://service.invalid/").send().await;

    assert_eq!(heard(first), Some(SOCKS5));
}

#[tokio::test]
async fn the_reqwest_of_youtube_music_greets_a_socks_proxy_in_socks() {
    let (address, first) = proxy();
    let client = reqwest_012::Client::builder()
        .proxy(reqwest_012::Proxy::all(address).unwrap())
        .build()
        .unwrap();

    let _ = client.get("http://service.invalid/").send().await;

    assert_eq!(heard(first), Some(SOCKS5));
}
