from scalecodec.base import RuntimeConfiguration, ScaleBytes
from scalecodec.type_registry import load_type_registry_preset

import hashlib

class BlockAnnounce():
    def __init__(self, notification):
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

        self.announce = RuntimeConfiguration().create_scale_object(
            'BlockAnnounce',
            data = ScaleBytes(notification),
        )
        self.announce.decode()
        print(self.announce['header'])

    """
        Get block header.
    """
    def header(self):
        return self.announce['header']

    """
        Get block number
    """
    def number(self):
        return int(self.announce['header']['number'].value_object)

    """
        Calculate block hash
    """
    def hash(self):
        header = RuntimeConfiguration().create_scale_object(
            'Header',
        )
        encoded = header.encode(self.announce['header'])
        byte_string = bytes.fromhex(encoded.to_hex()[2:])[:-2] # TODO: zzz
        hash_object = hashlib.blake2b(digest_size = 32)
        hash_object.update(byte_string)

        return str(hash_object.digest().hex())
