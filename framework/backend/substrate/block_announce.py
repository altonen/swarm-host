from scalecodec.base import RuntimeConfiguration, ScaleBytes
from scalecodec.type_registry import load_type_registry_preset

import hashlib
import copy

def init_runtime_config():
    runtime_config = RuntimeConfiguration()
    runtime_config.update_type_registry(load_type_registry_preset("legacy"))
    runtime_config.update_type_registry({
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

    return runtime_config


class BlockAnnounce():
    def __init__(self, runtime_config, notification):
        self.announce = runtime_config.create_scale_object(
            'BlockAnnounce',
            data = ScaleBytes(notification),
        )
        self.announce.decode()
        self.block_number = copy.deepcopy(self.announce['header']['number'])

    """
        Get block header.
    """
    def header(self):
        return self.announce['header']

    """
        Get block number
    """
    def number(self):
        return int(self.block_number.value_object)

    """
        Calculate block hash
    """
    def hash(self, runtime_config):
        header = runtime_config.create_scale_object(
            'Header',
        )
        encoded = header.encode(self.announce['header'])
        byte_string = bytes.fromhex(encoded.to_hex()[2:])[:-2] # TODO: zzz
        hash_object = hashlib.blake2b(digest_size = 32)
        hash_object.update(byte_string)

        return str(hash_object.digest().hex())
