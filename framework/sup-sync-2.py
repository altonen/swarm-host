from scalecodec.base import RuntimeConfiguration, ScaleBytes
from scalecodec.types import U128

import random
import json
from google.protobuf.internal import builder as _builder
from google.protobuf import descriptor as _descriptor
from google.protobuf import descriptor_pool as _descriptor_pool
from google.protobuf import symbol_database as _symbol_database
from enum import IntFlag
import struct
import proto

class Permissions(IntFlag):
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

def filter_request(ctx, peer, request):
    request_id = request['Request']['id']

    decoded = proto.BlockRequest()
    decoded.ParseFromString(bytes(request['Request']['payload']))

    print(decoded)

    RuntimeConfiguration().update_type_registry(load_type_registry_preset("core"))
    obj = RuntimeConfiguration().create_scale_object(
        'Hash',
        data = ScaleBytes(decoded.hash)
    )
    obj.decode()

    little_endian_bytes = struct.pack('<I', decoded.fields)
    little_endian_int = struct.unpack('>I', little_endian_bytes)[0]

    hash = str(obj)
    # key = "%s_%d" % (hash, little_endian_int)
    key = "%s" % (hash)
    # print("key", key)

    block = ctx.database.get(key)
    if block is not None:
        print("return block right away")
    else:
        for current_peer in ctx.peers:
            print("peer has", ctx.peers[current_peer].known_blocks)
            if hash in ctx.peers[current_peer].known_blocks:
                ctx.peers[current_peer].pending_request = True
                ctx.pending_requests[key] = [(peer, request_id)]
                print('forward request to', current_peer)
                print("save request for %s, requestId %d" % (str(peer), request_id))

                return {
                    'Request': {
                        'peer':  current_peer,
                        'payload': request['Request']['payload']
                    }
                }

    print("nobody found?")

    try:
        obj = RuntimeConfiguration().create_scale_object(
            'BlockRequest',
            data = ScaleBytes(bytes(request['Request']['payload'])),
        )
        obj.decode()

        start_from = obj['start_from'].value_object
        block =  ctx.database.get("block_%d" % (start_from))

        if block is not None:
            block = block.replace("'", "\"")
            block = json.loads(block)
            response = RuntimeConfiguration().create_scale_object(
                type_string = "BlockResponse"
            )
            encoded = response.encode({
                "blocks": [block],
            })
            byte_string = bytes.fromhex(encoded.to_hex()[2:])
            byte_list = [b for b in byte_string]

            return {
                'Response': [
                    {
                        'request_id': request_id,
                        'payload': byte_list,
                    }
                ]
            }
        else:
            if start_from in ctx.pending_requests:
                ctx.pending_requests[start_from].append((peer, request_id))
                return { 'DoNothing': None }

            for current_peer in ctx.peers:
                peer_best = ctx.peers[current_peer].best_block 
                if peer_best is not None and peer_best >= start_from and current_peer != peer:
                    if ctx.peers[peer].pending_request is True:
                        print("skip peer as it's already busy")
                        continue

                    ctx.peers[current_peer].pending_request = True
                    request = RuntimeConfiguration().create_scale_object(
                        type_string = "BlockRequest"
                    )
                    encoded = request.encode({
                        "start_from": start_from,
                        "num_blocks": 1,
                    })
                    byte_string = bytes.fromhex(encoded.to_hex()[2:])
                    byte_list = [b for b in byte_string]
                    # TODO: save the original request so it can be reconstructed?
                    ctx.pending_requests[start_from] = [(peer, request_id)]

                    return {
                        'Request': {
                            'peer':  current_peer,
                            'payload': byte_list,
                        }
                    }
        return { 'DoNothing': None }
    except Exception as e:
        print("failed to handle request", e)
        return { 'DoNothing': None }

def filter_response(ctx, peer, response):
    # mark peer as not busy
    ctx.peers[peer].pending_request = False

    decoded = proto.BlockResponse()
    decoded.ParseFromString(bytes(response['Response']['payload']))
    print(decoded)

    needed_block = None
    for block in decoded.blocks:
        obj = RuntimeConfiguration().create_scale_object(
            'Hash',
            data = ScaleBytes(block.hash)
        )
        obj.decode()

        key = "%s" % str(obj)

        if needed_block is None:
            needed_block = key

        repr = {
            'hash': str(block.hash),
            'header': str(block.header),
            'body': str(block.body)
        }
        ctx.database.set(key, str(repr))

    if needed_block in ctx.pending_requests:
        (peer, request_id) = ctx.pending_requests[needed_block][0]
        print("pending request exists for %s, peer %s" % ((str(needed_block), str(peer))))
        
        ctx.pending_requests[needed_block] = {}

        print("send response for", request_id)
        print(type(response))
        print(response)

        return {
            'Response': [{
                'request_id': request_id,
                'payload': response['Response']['payload']
            }]
        }
    else:
        print("do nothing", ctx.pending_requests, key, needed_block)
        return { 'DoNothing' : None }
