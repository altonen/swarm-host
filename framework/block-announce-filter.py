from backend.substrate.block_announce import BlockAnnounce, init_runtime_config

def inject_notification(
    ctx,
    peer,
    protocol,
    notification
):
    if ctx.runtime_config is None:
        ctx.runtime_config = init_runtime_config()

    if (datetime.datetime.now() - ctx.started).total_seconds() > 3 * 60:
        return

    try:
        block_announce = BlockAnnounce(ctx.runtime_config, bytes(notification))
        number = block_announce.number()
        hash = block_announce.hash(ctx.runtime_config)
        ctx.peers[peer].known_blocks.add(hash)
        ctx.set_block_hash(number, hash)

        forward_table = []
        for peer in ctx.peers:
            if hash not in ctx.peers[peer].known_blocks:
                forward_table.append(peer)

        if len(forward_table) != 0:
            ctx.forward_notification(protocol, forward_table, notification)
    except Exception as e:
        print("failed to handle block announce:", e)
