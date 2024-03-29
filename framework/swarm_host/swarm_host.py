from jsonrpcclient import parse, request
import requests

import logging
import subprocess
import time
import os

class SwarmHost:
    def __init__(self, rpc_port, backend, genesis_hash):
        self.logfile = open("/tmp/swarm-host-%d-%d" % (rpc_port, int(time.time())), "w")
        self.rpc_port = rpc_port
        args = [
            self.find_executable(),
            "--rpc-port", str(rpc_port),
            backend,
            "--genesis-hash", genesis_hash
        ]

        if "PYTHONPATH" not in os.environ:
            raise "`PYTHONPATH` must be defined"

        self.process = subprocess.Popen(
            args,
            stdout = self.logfile,
            stderr = subprocess.STDOUT,
            env = {
                "RUST_LOG": "overseer,mockchain,rpc,filter,executor::pyo3=trace,sub-libp2p=debug,filter::msg=OFF,executor::pyo3::msg=OFF",
                "PYTHONPATH": os.environ["PYTHONPATH"],
            }
        )
        time.sleep(2)

    def __del__(self):
        self.process.terminate()

    def find_executable(self):
        if "SWARM_HOST_EXECUTABLE" in os.environ:
            return os.environ["SWARM_HOST_EXECUTABLE"]

        hardcoded = [
            "../target/debug/swarm-host",
            "../target/release/swarm-host",
            "/usr/local/bin/swarm-host",
        ]

        for path in hardcoded:
            if os.path.exists(path):
                return path

        raise "Cannot find `swarm-host` executable"

    def create_interface(self, address, filter, preinit = None, poll_interval = 1000):
        logging.info("create interface %s" % (address))

        response = requests.post(
            "http://localhost:%d/" % (self.rpc_port),
            json=request(
                "create_interface",
                params=["127.0.0.1:%d" % address, filter, poll_interval, preinit],
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
            return response.json()["result"]
        elif "error" in response.json():
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
                params = [interface, protocol, filter, ctx],
            )
        )
        if "result" in response.json():
            print("success %s" % (response.json()["result"]))
            return response.json()["result"]
        elif "error" in response.json():
            print("failure %s" % (response.json()["error"]))
            return response.json()["error"]
