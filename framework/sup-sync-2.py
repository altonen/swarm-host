from scalecodec.base import RuntimeConfiguration, ScaleBytes
from scalecodec.types import U128

from google.protobuf.internal import builder as _builder
from google.protobuf import descriptor as _descriptor
from google.protobuf import descriptor_pool as _descriptor_pool
from google.protobuf import symbol_database as _symbol_database

from backend.substrate.block_request import BlockRequest, Attributes
from backend.substrate.block_response import BlockResponse

from enum import IntFlag, Enum

import json
import proto
import struct
import random

"""
    Inject request into filter.
"""
def inject_request(ctx, protocol, peer, request):
    request = BlockRequest(bytes(request['Request']['payload']))

    block_hash = request.hash()
    print("block reqeuest hash", block_hash)
    if len(block_hash) == 0:
        print("block hash is `None`, use number", request.number())
        number = int.from_bytes(request.number(), 'little')
        print("block number", number)
        block_hash = ctx.get_block_hash_from_number(number)
        if block_hash is None:
            return
    else:
        block_hash = block_hash.hex()

    # check if the block is already in the storage and if so, create a response right away
    block = ctx.database.get(block_hash)
    if block is not None:
        if request.max_blocks() == 1:
            ctx.create_and_send_response(peer, block)
        else:
            ctx.tmp(peer, block_hash, request.direction(), request.max_blocks())
        return

    # check if the request is already pending and if so, add peer to the table
    # of peers expecting a response and return early
    if block_hash in ctx.pending_requests:
        ctx.pending_requests[block_hash].append(peer)
        return

    # if the block is already cached, just mark that `peer` is expecting a response
    if block_hash in ctx.cached_requests:
        ctx.cached_requests[block_hash]['peers'].append(peer)
        return

    # get provider for the block
    provider = ctx.get_provider(block_hash)
    if provider is None and ctx.is_unknown_block(block_hash):
        print("unknown block", block_hash)
        return

    # if provider is `None` it means all peers that can provide the block
    # are busy and cannot answer the block request right now. cache the 
    # block request and send it later when one of the providers free up.
    if provider is None:
        ctx.cached_requests[block_hash] = { 'request': request, 'peers': [peer] }
        return

    # set the request as pending, mark the provider as busy and return
    # the request so the filter filter can forward it to `provider`
    ctx.pending_requests[block_hash] = [peer]
    ctx.peers[provider].busy = True

    print("return request for", provider)
    ctx.send_request(protocol, provider, request.to_bytes())

# inject response to filter
def inject_response(ctx, peer, response):
    response = BlockResponse(bytes(response['Response']['payload']))

    completed_pending_requests = []
    completed_cached_requests = []

    # mark the peer as not busy
    ctx.peers[peer].busy = False

    for block in response.blocks():
        # insert block to database if it doesn't exist
        block_hash = block.hash.hex()
        if ctx.database.get(block_hash) is None:
            ctx.save_block_to_database(block)

        # check if any peer is waiting `block`
        for pending_block in ctx.pending_requests:
            # if there are peers waiting for this block, mark the request as completed,
            # create a response and return it to all peers who are waiting for it
            if pending_block == block_hash:
                completed_pending_requests.append(block_hash)
                response = BlockResponse.new(block.hash, block.header, [body for body in block.body], block.justifications)

                for peer in ctx.pending_requests[pending_block]:
                    ctx.send_response(peer, response)

        # check if the response completed any cached requests
        for pending_block in ctx.cached_requests:
            if pending_block == block_hash:
                completed_cached_requests.append(block_hash)
                response = BlockResponse.new(block.hash, block.header, [body for body in block.body], block.justifications)

                for peer in ctx.cached_requests[pending_block]['peers']:
                    ctx.send_response(peer, response)

    # remove all completed pending requests
    for block_hash in completed_pending_requests:
        del ctx.pending_requests[block_hash]

    # remove all completed cached requests
    for block_hash in completed_cached_requests:
        del ctx.cached_requests[block_hash]
