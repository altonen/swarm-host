from substrateinterface import SubstrateInterface, Keypair
from substrateinterface.exceptions import SubstrateRequestException

from scalecodec.type_registry import load_type_registry_preset
from scalecodec.base import RuntimeConfiguration, ScaleBytes

import base64
import hashlib

from backend.substrate.block_announce import BlockAnnounce, init_runtime_config

def inject_notification(
    ctx,
    peer,
    notification,
):
    if ctx.block_announce_condition is not None:
        return ctx.block_announce_condition(ctx, peer, notification)
    else:
        return inject_notification(ctx, peer, notification)

def inject_notification(
    ctx,
    peer,
    notification
):
    if ctx.runtime_config is None:
        ctx.runtime_config = init_runtime_config()

    try:
        block_announce = BlockAnnounce(ctx.runtime_config, bytes(notification))
        number = block_announce.number()
        hash = block_announce.hash(ctx.runtime_config)
        ctx.peers[peer].known_blocks.add(hash)
        ctx.set_block_hash(number, hash)
    except Exception as e:
        print(number)
        print("failed to handle block announce:", e)
        return { 'Reject': None }

    return { 'Forward': None }
