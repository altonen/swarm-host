from jsonrpcclient import parse, request
import requests

import subprocess
import time

MOCKCHAIN_EXE = "../misc/mockchain/target/debug/mockchain"

class MockChain:
    def __init__(self, rpc_port, p2p_port, enable_gossip = False):
        self.logfile = open("/tmp/mock-%d-%d-%d" % (rpc_port, p2p_port, int(time.time())), "w")
        self.p2p_port = p2p_port
        self.rpc_port = rpc_port
        args = [
            MOCKCHAIN_EXE,
            "--p2p-port", str(p2p_port),
            "--rpc-port", str(rpc_port),
        ]

        if enable_gossip:
            args.append("--enable-gossip")

        self.process = subprocess.Popen(
            args,
            stdout=self.logfile,
            stderr=subprocess.STDOUT,
            env={"RUST_LOG": "overseer,p2p,rpc,gossip=trace"}
        )

    def __del__(self):
        self.process.terminate()

	# connect to remote node
    def connect(self, address):
        # TODO: params
        response = requests.post(
            "http://localhost:%d/" % (self.rpc_port),
            json=request(
                "connect",
                params=[address],
            )
        )
        if "result" in response.json():
            return response.json()["result"]
        elif "error" in response.json():
            return response.json()["error"]
        return None

	# get local address
    def get_local_address(self):
        response = requests.post("http://localhost:%d/" % (self.rpc_port), json=request("get_local_address"))
        return response.json()["result"]

	# get local peer id
    def get_local_peer_id(self):
        response = requests.post("http://localhost:%d/" % (self.rpc_port), json=request("get_local_peer_id"))
        return response.json()["result"]

	# get list of peers the local node is connected to
    def get_peers(self):
        return None
