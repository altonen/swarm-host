from substrateinterface import SubstrateInterface, Keypair
from substrateinterface.exceptions import SubstrateRequestException

from scalecodec.type_registry import load_type_registry_preset
from scalecodec.base import RuntimeConfiguration, ScaleBytes

import base64
import hashlib

def filter_notification(
    ctx,
    peer,
    notification
):
    # TODO: move this somewhere else?
    RuntimeConfiguration().update_type_registry(load_type_registry_preset("legacy"))
    RuntimeConfiguration().update_type_registry({
        "types": {
            "BlockAnnounce": {
                "type": "struct",
                "type_mapping": [
                    [
                        "header",
                        "Header",
                    ],
                    [
                        "state",
                        "BlockState",
                    ],
                    [
                        "data",
                        "Option<Vec<u8>>",
                    ],
                ],
            },
            "BlockState": {
                "type": "enum",
                "type_mapping": [
                    [
                        "Normal",
                        "Null",
                    ],
                    [
                        "Best",
                        "Null",
                    ],
                ]
            },
        }
    })

    try:
        obj = RuntimeConfiguration().create_scale_object(
            'BlockAnnounce',
            data = ScaleBytes(bytes(notification)),
        )
        obj.decode()
        test = RuntimeConfiguration().create_scale_object(
            'Header',
        )
        encoded = test.encode(obj['header'])
        # TODO: what are these two extra bytes at the end?
        byte_string = bytes.fromhex(encoded.to_hex()[2:])[:-2]
        hash_object = hashlib.blake2b(digest_size=32)
        hash_object.update(byte_string)
        ctx.peers[peer].known_blocks.add('0x' + str(hash_object.digest().hex()))
        return { 'Forward': None }
    except Exception as e:
        print("failed to handle block announce:", e)
        return { 'Reject': None }
