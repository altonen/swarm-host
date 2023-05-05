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

class Direction(IntFlag):
    ASCENDING = 0
    DESCENDING = 1

class BlockRequest():
    def __init__(self, request):
        self.original = request
        self.request = proto.BlockRequest()
        self.request.ParseFromString(request)

    """
        Get the original encoded request
    """
    def to_bytes(self):
        return self.original

    """
        Get flags of the received block request.
    """
    def flags(self):
        flags = struct.pack('<I', self.request.fields)
        return struct.unpack('>I', flags)[0]

    """
        Return the maximum number of blocks requested.
    """
    def max_blocks(self):
        return self.request.max_blocks

    def direction(self):
        if self.request.direction == Direction.ASCENDING:
            return Direction.ASCENDING
        return Direction.DESCENDING

    """
        Get block hash of the request
    """
    def hash(self):
        return self.request.hash

    """
        Get block number of the request
    """
    def number(self):
        return self.request.number
