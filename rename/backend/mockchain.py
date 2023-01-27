class MockChain:
    def __init__(self, rpc_port = 8887, p2p_port = 8888, enable_gossip = False):
    	self.rpc_port = 8887
    	self.p2p_port = 8888
    	self.enable_gossip = False

    	# TODO: start program with given attributes
    	# TODO: redirect console output to file in /tmp

    def connect(self, address, port):
        print("connect to peer")

    def address_info(self):
        print("query address info")
        return ("127.0.0.1", self.p2p_port)

    def peer_id(self):
        print("query peer id")

    def start(self):
        print("starting mockchain...")

    def verify_connected(self, peer):
        print("verify that peer connected")
