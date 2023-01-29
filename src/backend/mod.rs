use std::net::SocketAddr;

/// List of supported network backends.
#[derive(clap::ValueEnum, Clone)]
pub enum NetworkBackendType {
    Mockchain,
}

/// Traits which each network backend must implement.
pub trait NetworkBackend {
    fn start();
    fn connect(&mut self, address: SocketAddr) -> Result<(), ()>;
}
