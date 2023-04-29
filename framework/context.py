import redis

class Context():
    def __init__(self):
        self.database = redis.Redis(host = 'localhost', port = 6379, decode_responses = True)
        self.database.ping()
        self.peers = {}
        self.cached_requests = {}
        self.pending_requests = {}

    """
        Find a peer who is able to provide the requested block
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


class PeerContext():
    def __init__(self):
        self.known_blocks = set()
        self.busy = False

def initialize_ctx(ctx):
    return Context()

def register_peer(ctx, peer):
    ctx.peers[peer] = PeerContext()
