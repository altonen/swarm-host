from utils.swarm_host import SwarmHost
from backend.mockchain import MockChain

def main():
    print("initializing test program")

    mc1 = MockChain(8885, 8886, True)
    mc2 = MockChain(8887, 8888, False)

	# TOOD: these can be generalized
    (address, port) = mc2.address_info()
    mc1.connect(address, port)
    mc1.verify_connected(mc2.peer_id())

if __name__ == "__main__":
    main()
