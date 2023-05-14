from scalecodec.base import RuntimeConfiguration, ScaleBytes
from scalecodec.type_registry import load_type_registry_preset

from enum import IntFlag

class Roles(IntFlag):
	# No network.
	NONE = 0
	# Full node, does not participate in consensus.
	FULL = 1
	# Light client node.
	LIGHT = 2
	# Act as an authority
	AUTHORITY = 3

def initialize_interface():
    runtime_config = RuntimeConfiguration()
    runtime_config.update_type_registry(load_type_registry_preset("legacy"))
    runtime_config.update_type_registry({
        "types": {
            "BlockAnnouncesHandshake": {
                "type": "struct",
                "type_mapping": [
                    [
                        "roles",
                        "u32",
                    ],
                    [
                        "best_number",
                        "Compact<BlockNumber>",
                    ],
                    [
                        "best_hash",
                        "Hash",
                    ],
                    [
                        "genesis_hash",
                        "Hash",
                    ],
                ],
            },
        }
    })

    handshake = runtime_config.create_scale_object(
        'BlockAnnouncesHandshake',
    )
    encoded = handshake.encode({
        'roles': Roles.FULL,
        'best_number': 0,
        'best_hash': "0x36d171b4279dc05f16e8960afd9ae9e28cd620b610b84d6374949c099231c585",
        'genesis_hash': "0x36d171b4279dc05f16e8960afd9ae9e28cd620b610b84d6374949c099231c585",
    })

    return bytearray.fromhex(encoded.to_hex()[2:])
