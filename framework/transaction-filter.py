from substrateinterface import SubstrateInterface, Keypair
from substrateinterface.exceptions import SubstrateRequestException

from scalecodec.type_registry import load_type_registry_preset
from scalecodec.base import RuntimeConfiguration, ScaleBytes

import base64

# TODO: this needs to be a module

# Filter `notification` received from (`src_interface`, `src_peer`).
# 
# Check if `self.interface` should forward `notification` to  (`dst_interface`, `dst_peer`).
# Return `True` if `notification` should be forwarded and `False` if it should not be.
def filter_notification(
    ctx,
    src_interface,
    src_peer,
    dst_interface,
    dst_peer,
    notification
):
    base64_decoded_bytes = base64.b64decode(ctx)
    hex_string = base64_decoded_bytes.hex()
    original_hex_string = "0x" + hex_string

    RuntimeConfiguration().update_type_registry(load_type_registry_preset(name="core"))
    metadata = RuntimeConfiguration().create_scale_object(
        'MetadataVersioned',
        data = ScaleBytes(original_hex_string),
    )
    metadata.decode()
    RuntimeConfiguration().add_portable_registry(metadata)

    obj = RuntimeConfiguration().create_scale_object(
        'Vec<Extrinsic>',
        data = ScaleBytes(bytes(notification)),
        metadata = metadata,
    )
    obj.decode()
    print(obj)

    return True
