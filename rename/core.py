from utils.swarm_host import SwarmHost
from backend.mockchain import MockChain
import time

def main():
    print("initializing test program")

    mc1 = MockChain(8885, 8886, True)
    mc2 = MockChain(8887, 8888, False)

    time.sleep(2)

    mc1_peer_id = mc1.get_local_peer_id()
    mc1_address = mc1.get_local_address()

    print(mc1_peer_id)
    print(mc1_address)
    print(mc2.connect(mc1_address))

    time.sleep(15)

if __name__ == "__main__":
    main()
