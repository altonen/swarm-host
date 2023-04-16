import redis

class Context():
    def __init__(self):
        self.peers = {}
        self.pending_requests = {}
        self.database = redis.Redis(host = 'localhost', port = 6379, decode_responses = True)
        self.database.ping()
        self.database.flushall()

class PeerContext():
    def __init__(self):
        self.known_blocks = set()
        self.pending_request = False

def initialize_ctx(ctx):
    return Context()

def register_peer(ctx, peer):
    ctx.peers[peer] = PeerContext()
