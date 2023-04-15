use crate::backend::{
    mockchain::{types::RequestId, MockchainBackend},
    NetworkBackend, PacketSink,
};

use tokio::sync::mpsc;

mod filter;
mod notification;
mod request_response;

mockall::mock! {
    #[derive(Debug)]
    pub PacketSink<T: NetworkBackend> {}

    #[async_trait::async_trait]
    impl<T: NetworkBackend + Send> PacketSink<T> for PacketSink<T> {
        async fn send_packet(
            &mut self,
            protocol: Option<<T as NetworkBackend>::Protocol>,
            message: &<T as NetworkBackend>::Message,
        ) -> crate::Result<()>;
        async fn send_request(
            &mut self,
            protocol: <T as NetworkBackend>::Protocol,
            payload: Vec<u8>,
        ) -> crate::Result<<T as NetworkBackend>::RequestId>;
        async fn send_response(
            &mut self,
            request_id: <T as NetworkBackend>::RequestId,
            payload: Vec<u8>,
        ) -> crate::Result<()>;
    }
}

/// Dummy packet sink which only receives messages, only for testing
#[derive(Debug)]
struct DummyPacketSink<T: NetworkBackend> {
    msg_tx: mpsc::Sender<T::Message>,
    req_tx: mpsc::Sender<Vec<u8>>,
    resp_tx: mpsc::Sender<Vec<u8>>,
}

impl DummyPacketSink<MockchainBackend> {
    pub fn new() -> (
        Self,
        mpsc::Receiver<<MockchainBackend as NetworkBackend>::Message>,
        mpsc::Receiver<Vec<u8>>,
        mpsc::Receiver<Vec<u8>>,
    ) {
        let (msg_tx, msg_rx) = mpsc::channel(64);
        let (req_tx, req_rx) = mpsc::channel(64);
        let (resp_tx, resp_rx) = mpsc::channel(64);

        (
            Self {
                msg_tx,
                req_tx,
                resp_tx,
            },
            msg_rx,
            req_rx,
            resp_rx,
        )
    }
}

#[async_trait::async_trait]
impl PacketSink<MockchainBackend> for DummyPacketSink<MockchainBackend> {
    async fn send_packet(
        &mut self,
        protocol: Option<<MockchainBackend as NetworkBackend>::Protocol>,
        message: &<MockchainBackend as NetworkBackend>::Message,
    ) -> crate::Result<()> {
        self.msg_tx.send(message.clone()).await.unwrap();
        Ok(())
    }

    async fn send_request(
        &mut self,
        protocol: <MockchainBackend as NetworkBackend>::Protocol,
        payload: Vec<u8>,
    ) -> crate::Result<<MockchainBackend as NetworkBackend>::RequestId> {
        self.req_tx.send(payload).await.unwrap();
        Ok(RequestId(0u64))
    }

    async fn send_response(
        &mut self,
        request_id: <MockchainBackend as NetworkBackend>::RequestId,
        payload: Vec<u8>,
    ) -> crate::Result<()> {
        self.resp_tx.send(payload).await.unwrap();
        Ok(())
    }
}
