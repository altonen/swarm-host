# class Filter():
#     def __init__(self, interface):
#         self.interface = interface
#         pass

#     def filter_request(self, src_interface, src_peer, request):
#         pass

#     def filter_response(self, src_interface, src_peer, request):
#         pass

# class MyCustomFilter(Filter):
#     def __init__(self, interface):
#         super.__init__(interface)
    
# # Initialize context for the filter.
# #
# # Get the interface ID as parameter which the context can store 
# def initialize_filter(interface):
#     return Filter(interface)
class Context():
    def __init__(self):
        self.peers = {}

class PeerContext():
    def __init__(self):
        self.known_blocks = set()
        self.pending_request = False

def initialize_ctx(ctx):
    return Context()

def register_peer(ctx, peer):
    ctx.peers[peer] = PeerContext()
