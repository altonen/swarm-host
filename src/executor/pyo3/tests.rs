use crate::{
    backend::mockchain::{types::ProtocolId, MockchainBackend},
    executor::{pyo3::PyO3Executor, Executor, NotificationHandlingResult},
};

use rand::Rng;

#[test]
fn drop_notification() {
    let context_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();
    let notification_filter_code = "
def inject_notification(ctx, peer, notification):
    if peer is not None:
        print('peer %d' % (peer))
    print(notification)
    return {'Drop': None}
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let mut executor = <PyO3Executor<MockchainBackend> as Executor<MockchainBackend>>::new(
        rng.gen(),
        context_code,
        None,
    )
    .unwrap();

    <PyO3Executor<MockchainBackend> as Executor<MockchainBackend>>::install_notification_filter(
        &mut executor,
        ProtocolId::Transaction,
        notification_filter_code,
    )
    .unwrap();

    let result =
        <PyO3Executor<MockchainBackend> as Executor<MockchainBackend>>::inject_notification(
            &mut executor,
            &ProtocolId::Transaction,
            rng.gen(),
            rand::random(),
        );

    assert_eq!(result, Ok(NotificationHandlingResult::Drop));
}
