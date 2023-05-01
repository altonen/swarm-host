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
        'best_hash': "0xf4dc5c992f828d4e6081ff6e7cc6d933ae1d9d3fdadd9f540b368580015abb4f",
        'genesis_hash': "0xf4dc5c992f828d4e6081ff6e7cc6d933ae1d9d3fdadd9f540b368580015abb4f",
    })

    return bytearray.fromhex(encoded.to_hex()[2:])
