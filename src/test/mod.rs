mod client;
mod server;

use std::net::TcpListener;



#[test]
fn main() {
	let server = TcpListener::bind("0.0.0.0:7087").unwrap();
}