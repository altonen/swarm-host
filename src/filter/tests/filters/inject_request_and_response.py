from scalecodec.base import RuntimeConfiguration, ScaleBytes
from scalecodec.types import U128

import random
import json

def filter_request(ctx, peer, request):
    RuntimeConfiguration().update_type_registry({
        "types": {
            "BlockRequest": {
                "type": "struct",
                "type_mapping": [
                    [
                        "start_from",
                        "u128",
                    ],
                    [
                        "num_blocks",
                        "u8",
                    ],
                ],
            },
            "BlockResponse": {
                "type": "struct",
                "type_mapping": [
                    [
                        "blocks",
                        "Vec<Block>",
                    ],
                ],
            },
            "Block": {
                "type": "struct",
                "type_mapping": [
                    [
                        "number",
                        "u128",
                    ],
                    [
                        "time",
                        "u64",
                    ],
                    [
                        "transactions",
                        "Vec<Transaction>",
                    ],
                ],
            },
            "Transaction": {
                "type": "struct",
                "type_mapping": [
                    [
                        "sender",
                        "u64",
                    ],
                    [
                        "receiver",
                        "u64",
                    ],
                    [
                        "amount",
                        "u64",
                    ],
                ],
            },
        }
    })

    request_id = request['Request']['id']

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
                        'peer': peer,
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
    RuntimeConfiguration().update_type_registry({
        "types": {
            "BlockResponse": {
                "type": "struct",
                "type_mapping": [
                    [
                        "blocks",
                        "Vec<Block>",
                    ],
                ],
            },
            "Block": {
                "type": "struct",
                "type_mapping": [
                    [
                        "number",
                        "u128",
                    ],
                    [
                        "time",
                        "u64",
                    ],
                    [
                        "transactions",
                        "Vec<Transaction>",
                    ],
                ],
            },
            "Transaction": {
                "type": "struct",
                "type_mapping": [
                    [
                        "sender",
                        "u64",
                    ],
                    [
                        "receiver",
                        "u64",
                    ],
                    [
                        "amount",
                        "u64",
                    ],
                ],
            },
        }
    })

    # mark peer as not busy
    if peer in ctx.peers:
        ctx.peers[peer].pending_request = False

    try:
        response = RuntimeConfiguration().create_scale_object(
            'BlockResponse',
            data = ScaleBytes(bytes(response['Response']['payload'])),
        )
        response.decode()

        responses = []
        for block in response['blocks']:
            ctx.database.set(("block_%d" % (block['number'].value_object)), str(block))

            if block['number'].value_object in ctx.pending_requests:
                response = RuntimeConfiguration().create_scale_object(
                    type_string = "BlockResponse"
                )
                encoded = response.encode({
                    "blocks": [block],
                })

                # TODO: what is this extra byte?
                byte_string = bytes.fromhex(encoded.to_hex()[2:])[1:]

                responses = []
                for (peer, request_id) in ctx.pending_requests[block['number'].value_object]:
                    responses.append({
                        'peer': peer,
                        'payload': byte_string,
                    })
                ctx.pending_requests[block['number'].value_object] = None

                return {"Response": {
                        "Responses": responses,
                        "Request": None,
                    },
                }
        return { 'DoNothing' : None }
    except Exception as e:
        print("failed to decode block response:", e)
        return { 'DoNothing' : None }
