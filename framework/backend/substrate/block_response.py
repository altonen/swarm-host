from scalecodec.base import RuntimeConfigurationObject, ScaleBytes, RuntimeConfiguration
from scalecodec.type_registry import load_type_registry_preset

from enum import IntFlag

import proto
import struct

class Attributes(IntFlag):
	# Include block header.
	HEADER = 1
	# Include block body.
	BODY = 2
	# Include block receipt.
	RECEIPT = 4
	# Include block message queue.
	MESSAGE_QUEUE = 8
	# Include a justification for the block.
	JUSTIFICATION = 16

class BlockResponse():
    def __init__(self, response):
        self.original = response
        self.response = proto.BlockResponse()
        self.response.ParseFromString(response)

    def new(hash, header, body, justification):
        block_data = proto.BlockData()
        block_data.hash = hash
        block_data.header = header

        if body is not None:
            for body in body:
                block_data.body.append(body)

        if justification is not None:
            block_data.justification = justification

        response = proto.BlockResponse()
        response.blocks.append(block_data)
        return response.SerializeToString()

    def new_from_blocks(blocks):
        response = proto.BlockResponse()

        for hash, header, body, justification in blocks:
            block_data = proto.BlockData()
            block_data.hash = hash
            block_data.header = header

            if body is not None:
                for body in body:
                    block_data.body.append(body)

            if justification is not None:
                block_data.justification = justification

            response.blocks.append(block_data)

        return response.SerializeToString()

    def blocks(self):
        return self.response.blocks

    def to_bytes(self):
        return self.original
