from substrateinterface import SubstrateInterface, Keypair
from substrateinterface.exceptions import SubstrateRequestException

import logging
import subprocess
import time

class InvalidConfiguration(Exception):
    pass

class Node():
    """
        Start a generic Substrate node.
    """
    def __init__(self, type_registry_preset, default_path):
        self.type_registry_preset = type_registry_preset
        self.path = default_path
        self.exec_arguments = []

    """
        Specify P2P port.
    """
    def with_p2p_port(self, p2p_port):
        logging.debug("rpc port: `%d`" % (p2p_port))

        self.exec_arguments.append("--port")
        self.exec_arguments.append(str(p2p_port))
        return self

    """
        Specify RPC port.
    """
    def with_rpc_port(self, rpc_port):
        logging.debug("rpc port: `%d`" % (rpc_port))

        self.exec_arguments.append("--rpc-port")
        self.exec_arguments.append(str(rpc_port))
        return self

    def with_binary_path(self, path):
        logging.debug("binary path: `%s`" % (path))

        self.path = path
        return self

    """
        Specify base path.
    """
    def with_base_path(self, path = None, tmp = None):
        if tmp == True and path is None:
            logging.debug("base path: `--tmp`")
            self.exec_arguments.append("--tmp")
        elif path is not None and tmp is None:
            logging.debug("base path: `%s`" % (path))
            self.exec_arguments.append("--base-path")
            self.exec_arguments.append(path)
        else:
            raise InvalidConfiguration("`path` and `tmp` are mutually exclusive")
        return self

    def build(self):
        logging.info("launch node: path {}, arguments {}".format(self.path, self.exec_arguments))

        self.logfile = open(
            "/tmp/%s-%d" % (
                self.type_registry_preset,
                int(time.time())
            ),
            "w"
        )
        args = [self.path] + self.exec_arguments
        self.process = subprocess.Popen(
            args,
            stdout=self.logfile,
            stderr=subprocess.STDOUT,
            env={"RUST_LOG": "sub-libp2p=debug,info"}
        )
        print("hello")

        # TODO: start node here and wait for a second in order to let it start
        # self.substrate = SubstrateInterface(
        #     url = "ws://127.0.0.1:%d" % (rpc_port),
        #     type_registry_preset = type_registry_preset,
        # )
        # TODO: get chain metadata and return the SCALE-encoded object
        return self

    def __del__(self):
        print("destroy node")
        self.process.terminate()

    """
        Get chain metadata.
    """
    def get_metadata(self):
        pass

    """
        Submit extrinsic.
    """
    def submit_extrinsic(self, extrinsic):
        print("submit extrinsic '{}'".format(extrinsic))
        pass

class Polkadot(Node):
    """
        Start Polkadot node.
    """
    def __init__(self):
        super().__init__(type_registry_preset = "polkadot", default_path = "/usr/local/bin/polkadot")

class NodeTemplate(Node):
    """
        Start template node.
    """
    def __init__(self):
        super().__init__(type_registry_preset = "node-template", default_path = "/usr/local/bin/node-template")
