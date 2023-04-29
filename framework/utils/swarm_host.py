from jsonrpcclient import parse, request
import requests

import logging
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
            "--backend", backend,
        ]

        self.process = subprocess.Popen(
            args,
            stdout = self.logfile,
            stderr = subprocess.STDOUT,
            env = {"RUST_LOG": "overseer,mockchain,rpc,filter=trace,sub-libp2p=debug,filter::msg=off"}
        )

    def __del__(self):
        self.process.terminate()

    def create_interface(self, address):
        logging.info("create interface %s" % (address))

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
        logging.info("link interfaces %d %d" % (iface1_id, iface2_id))

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
        logging.info("unlink interface")

    """
        Initialize filter context for the interface.
    """
    def install_context_filter(self, interface, filter, ctx = ""):
        response = requests.post(
            "http://localhost:%d/" % (self.rpc_port),
            json = request(
                "initialize_filter",
                params = [interface, filter, ctx],
            )
        )

    """
        Install filter for protocol.
    """
    def install_notification_filter(self, interface, protocol, filter, ctx = ""):
        logging.info("install notification filter for %s" % (protocol))

        response = requests.post(
            "http://localhost:%d/" % (self.rpc_port),
            json = request(
                "install_notification_filter",
                params = [interface, protocol, filter, ctx],
            )
        )
        if "result" in response.json():
            print("success %s" % (response.json()["result"]))
            return response.json()["result"]
        elif "error" in response.json():
            print("failure %s" % (response.json()["error"]))
            return response.json()["error"]

    """
        Install request-response filter.
    """
    def install_request_response_filter(self, interface, protocol, filter, ctx = ""):
        logging.info("install request-response filter for %s" % (protocol))

        response = requests.post(
            "http://localhost:%d/" % (self.rpc_port),
            json = request(
                "install_request_response_filter",
                params = [0, protocol, filter, ctx],
            )
        )
        if "result" in response.json():
            print("success %s" % (response.json()["result"]))
            return response.json()["result"]
        elif "error" in response.json():
            print("failure %s" % (response.json()["error"]))
            return response.json()["error"]
