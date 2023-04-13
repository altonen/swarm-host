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
    peer,
    notification
):
    return { 'Forward': None }
    # TODO: move this somewhere else?
    # RuntimeConfiguration().update_type_registry(load_type_registry_preset("legacy"))
    # RuntimeConfiguration().update_type_registry({
    #     "types": {
    #         "BlockAnnounce": {
    #             "type": "struct",
    #             "type_mapping": [
    #                 [
    #                     "header",
    #                     "Header",
    #                 ],
    #                 [
    #                     "state",
    #                     "BlockState",
    #                 ],
    #                 [
    #                     "data",
    #                     "Option<Vec<u8>>",
    #                 ],
    #             ],
    #         },
    #         "BlockState": {
    #             "type": "enum",
    #             "type_mapping": [
    #                 [
    #                     "Normal",
    #                     "Null",
    #                 ],
    #                 [
    #                     "Best",
    #                     "Null",
    #                 ],
    #             ]
    #         },
    #     }
    # })

    # try:
    #     obj = RuntimeConfiguration().create_scale_object(
    #         'BlockAnnounce',
    #         data = ScaleBytes(bytes(notification)),
    #     )
    #     obj.decode()
    # except:
    #     print("failed to decode block announce")

    # return True
