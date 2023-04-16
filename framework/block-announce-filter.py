from substrateinterface import SubstrateInterface, Keypair
from substrateinterface.exceptions import SubstrateRequestException

from scalecodec.type_registry import load_type_registry_preset
from scalecodec.base import RuntimeConfiguration, ScaleBytes

import base64

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
        ctx.peers[peer].known_blocks.add(str(obj['header']))
        return { 'Forward': None }
    except Exception as e:
        print("failed to handle block announce:", e)
        return { 'Reject': None }
