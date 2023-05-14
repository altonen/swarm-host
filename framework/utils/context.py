from scalecodec.base import RuntimeConfiguration

import redis

# maximum peers each interface can have
MAX_PEERS = 400

class PeerContext():
    def __init__(self):
        self.known_blocks = set()
        self.busy = False

class Context():
    def __init__(self):
        self.database = redis.Redis(host = 'localhost', port = 6379, decode_responses = True)
        self.database.ping()
        self.peers = {}
        self.number_to_block_hash = {}
        self.cached_requests = {}
        self.pending_requests = {}
        self.runtime_config = None
        self.peer_events = []
        self.pending_peers = set()
        self.known_peers = set()

    """
        Map block number to block hash.
    """
    def set_block_hash(self, number, hash):
        if number not in self.number_to_block_hash:
            self.number_to_block_hash[number] = hash

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
    def get_cached_request(ctx):
        block = None
        for cached in ctx.cached_requests:
            provider = ctx.get_provider(cached)
            if provider is not None:
                block = cached
                break

        if block is None:
            return None

        # if a cached request can be sent to some peer, schedule requests for
        # every peer that is waiting for the cached request to complete
        request = ctx.cached_requests[block]['request']
        peers = ctx.cached_requests[block]['peers']
        del ctx.cached_requests[block]

        result = None
        for peer in peers:
            if result is None:
                result = ctx.inject_request(peer, request)
            else:
                ctx.inject_request(peer, request)
        return result

    """
        Create `BlockResponse` from a block and return it to user.
    """
    def return_response(self, peer, block):
        print("try to return response to %s for block %s" % (peer, block))
        block = json.loads(block)

        justification = block.get('justification')
        if justification is not None:
            justification = bytes.fromhex(justification)

        body = block.get('body')
        if body is not None:
            body = [bytes.fromhex(body) for body in body]

        response = BlockResponse.new(
            bytes.fromhex(block['hash']),
            bytes.fromhex(block['header']),
            body,
            justification,
        )

        return { 'Response': [{
                'peer': peer,
                'payload': response,
            }]
        }

    def register_peer(self: Context, peer):
        self.peers[peer] = PeerContext()

        if peer in self.pending_peers:
            self.pending_peers.remove(peer)

    def unregister_peer(self, peer):
        if peer in self.peers:
            del self.peers[peer]

    def discover_peer(self: Context, peer):
        if peer in self.pending_peers or peer in self.peers:
            return

        self.known_peers.add(peer)
        needed_peers = MAX_PEERS - len(self.peers) + len(self.pending_peers)
        if needed_peers <= 0:
            return

        selected = []
        npeers = min(needed_peers, len(self.known_peers))
        for peer in list(self.known_peers)[:npeers]:
            self.known_peers.remove(peer)
            self.pending_peers.add(peer)
            self.peer_events.append({ 'Connect': peer })

    def poll(self: Context):
        pending_events = []
        pending_events.extend(self.peer_events)
        self.peer_events.clear()

        return pending_events