from scalecodec.base import RuntimeConfiguration

from backend.substrate.block_response import BlockResponse
from backend.substrate.block_request import Direction

import redis

# maximum peers each interface can have
MAX_PEERS = 400

class Context():
    def __init__(self):
        self.database = redis.Redis(host = 'localhost', port = 6379, decode_responses = True)
        self.database.ping()
        self.peers = {}
        self.number_to_block_hash = {}
        self.block_hash_to_number = {}
        self.cached_requests = {}
        self.pending_requests = {}
        self.runtime_config = None
        self.pending_peers = set()
        self.known_peers = set()
        self.pending_events = []

    """
        Map block number to block hash.
    """
    def set_block_hash(self, number, hash):
        if number not in self.number_to_block_hash:
            self.number_to_block_hash[number] = hash
        if hash not in self.block_hash_to_number:
            self.block_hash_to_number[hash] = number

    """
        Get block hash with a block number.
    """
    def get_block_hash_from_number(self, number):
        if number in self.number_to_block_hash:
            return self.number_to_block_hash[number]

    """
        Find a peer who is able to provide the requested block.
    """
    def get_provider(ctx, block):
        for peer in ctx.peers:
            print(ctx.peers[peer].known_blocks)
            if block in ctx.peers[peer].known_blocks and not ctx.peers[peer].busy:
                return peer

    """
        Check if the requested block is known by any connected peers.
    """
    def is_unknown_block(ctx, block):
        for peer in ctx.peers:
            if block in ctx.peers[peer].known_blocks:
                return False
        return True

    # construct a dictionary of the block response, convert it to JSON and save it to database
    def save_block_to_database(ctx, block):
        encoded = { 'hash': block.hash.hex() }
        if block.header != b'':
            encoded['header'] = block.header.hex()
        if block.body != b'':
            encoded['body'] = [body.hex() for body in block.body]
        if block.justification != b'':
            encoded['justification'] = block.justification.hex()

        ctx.database.set(block.hash.hex(), json.dumps(encoded))

    # attempt to schedule cached request for sending
    def send_cached_request(ctx):
        block = None
        for cached in ctx.cached_requests:
            provider = ctx.get_provider(cached)
            if provider is not None:
                block = cached
                break

        if block is None:
            return

        # if a cached request can be sent to some peer, schedule requests for
        # every peer that is waiting for the cached request to complete
        request = ctx.cached_requests[block]['request']
        peers = ctx.cached_requests[block]['peers']
        del ctx.cached_requests[block]

    """
        Create `BlockResponse` and push it to pending events
    """
    def create_and_send_response(self, peer, start_hash, direction, max_blocks):
        if start_hash not in self.block_hash_to_number:
            raise Exception("mapping between block hash and number doesn not exist")

        start_number = self.block_hash_to_number[start_hash]
        block_data = []
        block_number = start_number

        while True:
            if block_number not in self.number_to_block_hash:
                break

            block_hash = self.number_to_block_hash[block_number]
            block = self.database.get(block_hash)
            block = json.loads(block)

            justification = block.get('justification')
            if justification is not None:
                justification = bytes.fromhex(justification)

            body = block.get('body')
            if body is not None:
                body = [bytes.fromhex(body) for body in body]

            block_data.append((
                bytes.fromhex(block['hash']),
                bytes.fromhex(block['header']),
                body,
                justification,
            ))

            max_blocks -= 1
            if max_blocks == 0:
                break
            if direction == Direction.ASCENDING:
                block_number += 1
            else:
                block_number -= 1

        self.pending_events.append({ 'SendResponse': {
                'peer': peer,
                'response': BlockResponse.from_blocks(block_data),
            }
        })

    """
        Forward `notification` to `peers`.
    """
    def forward_notification(self, protocol, peers, notification):
        self.pending_events.append({ 'Forward': {
                'peers': peers,
                'protocol': protocol,
                'notification': notification,
            }
        })

    """
        Send request to peer.
    """
    def send_request(self, protocol, peer, request):
        self.pending_events.append({ 'SendRequest': {
                'protocol': protocol,
                'peer': peer,
                'request': request
            }
        })

    """
        Send response to peer.
    """
    def send_response(self, peer, response):
        self.pending_events.append({ 'SendResponse': {
                'peer': peer,
                'response': response
            }
        })

class PeerContext():
    def __init__(self):
        self.known_blocks = set()
        self.busy = False

def initialize_ctx(ctx):
    return Context()

def register_peer(ctx: Context, peer):
    ctx.peers[peer] = PeerContext()

    if peer in ctx.pending_peers:
        ctx.pending_peers.remove(peer)

def unregister_peer(ctx: Context, peer):
    if peer in ctx.peers:
        del ctx.peers[peer]
        ctx.known_peers.add(peer)

def discover_peer(ctx: Context, peer):
    if peer in ctx.pending_peers or peer in ctx.peers:
        return

    ctx.known_peers.add(peer)
    needed_peers = MAX_PEERS - len(ctx.peers) + len(ctx.pending_peers)
    if needed_peers <= 0:
        return

    selected = []
    npeers = min(needed_peers, len(ctx.known_peers))
    for peer in list(ctx.known_peers)[:npeers]:
        ctx.known_peers.remove(peer)
        ctx.pending_peers.add(peer)
        ctx.pending_events.append({ 'Connect': peer })

def poll(ctx: Context):
    pending_events = ctx.pending_events.copy()
    ctx.pending_events.clear()

    return pending_events
