from jsonrpcclient import parse, request
import requests

import subprocess
import time

MOCKCHAIN_EXE = "../target/debug/swarm-host"

class SwarmHost:
    def __init__(self, rpc_port, backend):
        self.logfile = open("/tmp/swarm-host-%d-%d" % (rpc_port, int(time.time())), "w")
        self.rpc_port = rpc_port
        args = [
            MOCKCHAIN_EXE,
            "--rpc-port", str(rpc_port),
            "--backend", backend
        ]

        self.process = subprocess.Popen(
            args,
            stdout=self.logfile,
            stderr=subprocess.STDOUT,
            env={"RUST_LOG": "overseer,mockchain,rpc,filter=trace"}
        )

    def __del__(self):
        self.process.terminate()

    def create_interface(self, address):
        print("create interface %s" % (address))

        response = requests.post(
            "http://localhost:%d/" % (self.rpc_port),
            json=request(
                "create_interface",
                params=[address],
            )
        )
        if "result" in response.json():
            return response.json()["result"]
        elif "error" in response.json():
            return response.json()["error"]
        return None

    def link_interface(self, iface1_id, iface2_id):
        print("link interfaces %d %d" % (iface1_id, iface2_id))

        response = requests.post(
            "http://localhost:%d/" % (self.rpc_port),
            json=request(
                "link_interface",
                params=[iface1_id, iface2_id],
            )
        )
        if "result" in response.json():
            return response.json()["result"]
        elif "error" in response.json():
            return response.json()["error"]
        return None

    def unlink_interace(self):
        print("unlink interface")
