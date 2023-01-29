use std::net::SocketAddr;

// TODO: define common interface

pub trait Backend {
    fn start();
    fn connect(&mut self, address: SocketAddr) -> Result<(), ()>;
}
