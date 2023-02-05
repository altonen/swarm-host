from utils.swarm_host import SwarmHost
from backend.mockchain import MockChain
import time

def main():
    print("initializing test program")
    host = SwarmHost(8884, "mockchain")
    mc1 = MockChain(8885, 8886, True)
    mc2 = MockChain(8887, 8888, False)

    time.sleep(2)

    iface1_addr = "127.0.0.1:4444"
    iface1_id = host.create_interface(iface1_addr)
    print("interface id '%d'" % iface1_id)

    iface2_addr = "127.0.0.1:5555"
    iface2_id = host.create_interface(iface2_addr)
    print("interface id '%d'" % iface2_id)

	# link interfaces and connect peers to interfaces
    host.link_interface(iface1_id, iface2_id)

    mc1.connect(iface1_addr)
    mc2.connect(iface2_addr)

    time.sleep(15)

if __name__ == "__main__":
    main()
