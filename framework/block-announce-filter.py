from substrateinterface import SubstrateInterface, Keypair
from substrateinterface.exceptions import SubstrateRequestException

from scalecodec.type_registry import load_type_registry_preset
from scalecodec.base import RuntimeConfiguration, ScaleBytes

import base64
import hashlib

from backend.substrate.block_announce import BlockAnnounce

def filter_notification(
    ctx,
    peer,
    notification
):
    try:
        block_announce = BlockAnnounce(bytes(notification))
        ctx.peers[peer].known_blocks.add(block_announce.hash())
        ctx.set_block_hash(block_announce.number(), block_announce.hash())
        return { 'Forward': None }
    except Exception as e:
        print("failed to handle block announce:", e)
        return { 'Reject': None }
