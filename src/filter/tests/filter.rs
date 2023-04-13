use crate::{
    backend::{
        mockchain::{
            types::{PeerId, ProtocolId},
            MockchainBackend,
        },
        NetworkBackend, PacketSink,
    },
    error::Error,
    executor::pyo3::PyO3Executor,
    filter::Filter,
};

use rand::Rng;
use tokio::sync::mpsc;

// TODO: move somewhere else maybe
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
            response: <T as NetworkBackend>::Request,
        ) -> crate::Result<<T as NetworkBackend>::RequestId>;
        async fn send_response(
            &mut self,
            request_id: <T as NetworkBackend>::RequestId,
            response: <T as NetworkBackend>::Response,
        ) -> crate::Result<()>;
    }
}

#[test]
fn initialize_filter() {
    let filter_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, filter_code, None)
        .is_ok());
}

#[test]
fn context_initialization_function_missing() {
    let filter_code = "
def invalid_name_for_context_initialization_function(ctx):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, filter_code, None)
        .is_err());
}

#[test]
fn install_notification_filter() {
    let context_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();
    let notification_filter_code = "
def filter_notification(ctx, peer, notification):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, context_code, None)
        .is_ok());
    assert!(filter
        .install_notification_filter(ProtocolId::Transaction, notification_filter_code)
        .is_ok());
}

#[test]
fn notification_filter_missing() {
    let context_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();
    let filter_code = "
def __filter_notification__(ctx):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, context_code, None)
        .is_ok());
    assert!(filter
        .install_notification_filter(ProtocolId::Transaction, filter_code)
        .is_err());
}

#[test]
fn inject_notification() {
    let context_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();
    let notification_filter_code = "
def filter_notification(ctx, peer, notification):
    if peer is not None:
        print('peer %d' % (peer))
    print(notification)
    return { 'Drop': None }
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, context_code, None)
        .is_ok());
    assert!(filter
        .install_notification_filter(ProtocolId::Transaction, notification_filter_code)
        .is_ok());
    assert!(filter
        .inject_notification(&ProtocolId::Transaction, rng.gen(), rand::random())
        .is_ok());
}

#[test]
fn register_peer() {
    let filter_code = "
class Context():
    def __init__(self):
        self.peers = {}

def initialize_ctx(ctx):
    return Context()

def register_peer(ctx, peer):
    print('register peer %d to filter' % (peer))
    ctx.peers[peer] = peer
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor>::new(interface, tx);
    let mock_sink = Box::new(MockPacketSink::new());

    assert!(filter
        .initialize_filter(interface, filter_code, None)
        .is_ok());
    assert!(filter.register_peer(rng.gen(), mock_sink).is_ok());
}

#[test]
fn unregister_peer() {
    let filter_code = "
class Context():
    def __init__(self):
        self.peers = {}

def initialize_ctx(ctx):
    return Context()

def register_peer(ctx, peer):
    print('register peer %d to filter' % (peer))
    ctx.peers[peer] = peer

def unregister_peer(ctx, peer):
    print('unregister peer %d from filter' % (peer))
    if peer in ctx.peers:
        del ctx.peers[peer]
    else:
        print('peer does not exist')
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let peer = rng.gen();
    let interface = rng.gen();
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor>::new(interface, tx);
    let mock_sink = Box::new(MockPacketSink::new());

    assert!(filter
        .initialize_filter(interface, filter_code, None)
        .is_ok());
    assert!(filter.register_peer(peer, mock_sink).is_ok());
    assert!(filter.unregister_peer(peer).is_ok());
    assert!(filter.unregister_peer(peer).is_err());
}

#[test]
fn delay_notification() {
    let context_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();
    let notification_filter_code = "
def filter_notification(ctx, peer, notification):
    if peer is not None:
        print('peer %d' % (peer))
    print(notification)
    return { 'Delay': 15 }
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, context_code, None)
        .is_ok());
    assert!(filter
        .install_notification_filter(ProtocolId::Transaction, notification_filter_code)
        .is_ok());

    // inject the notification to a filter that delays it
    // verify that the notification is added to the list of delayed notifications
    assert!(filter.delayed_notifications.is_empty());
    assert!(filter
        .inject_notification(&ProtocolId::Transaction, rng.gen(), rand::random())
        .is_ok());
    assert_eq!(filter.delayed_notifications.len(), 1);
}
