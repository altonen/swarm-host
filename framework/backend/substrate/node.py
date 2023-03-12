from substrateinterface import SubstrateInterface, Keypair
from substrateinterface.exceptions import SubstrateRequestException

from scalecodec.type_registry import load_type_registry_preset
from scalecodec.base import RuntimeConfiguration, ScaleBytes

from os.path import exists
import os
import logging
import subprocess
import time

# TODO: add ability to query rpc and ws ports

class InvalidConfiguration(Exception):
    pass

class RpcQueryError(Exception):
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
        self.p2p_port = p2p_port
        return self

    """
        Specify RPC port.
    """
    def with_rpc_port(self, rpc_port):
        logging.debug("rpc port: `%d`" % (rpc_port))

        self.exec_arguments.append("--rpc-port")
        self.exec_arguments.append(str(rpc_port))
        self.rpc_port = rpc_port
        return self

    """
        Specify WebSocket RPC port.
    """
    def with_ws_port(self, ws_port):
        logging.debug("ws port: `%d`" % (ws_port))

        self.exec_arguments.append("--ws-port")
        self.exec_arguments.append(str(ws_port))
        self.ws_port = ws_port
        return self

    def with_binary_path(self, path):
        logging.debug("binary path: `%s`" % (path))

        self.path = path
        return self

    """
        Specify base path.
    """
    def with_base_path(self, path = None, tmp = False):
        if tmp is True and path is None:
            logging.debug("base path: `--tmp`")
            self.exec_arguments.append("--tmp")
        elif path is not None and tmp is True:
            logging.debug("base path: `%s`" % (path))
            self.exec_arguments.append("--base-path")
            self.exec_arguments.append(path)
        else:
            raise InvalidConfiguration("`path` and `tmp` are mutually exclusive")
        return self

    """
        Mark node as validator.
    """
    def as_validator(self):
        logging.debug("mark node as validator")
        self.exec_arguments.append("--validator")
        return self

    """
        Provide profile.

        The given profile must be one of the following:
            - `alice`
            - `bob`
            - `charlie`
            - `dave`
            - `even`
            - `ferdie`
    """
    def with_profile(self, profile):
        allowed_profiles = ["alice", "bob", "charlie", "dave", "eve", "ferdie"]
        if profile not in allowed_profiles:
            raise InvalidConfiguration("`%s` not in allowed profiles" % (profile))

        logging.debug("profile: `%s`" % (profile))
        self.exec_arguments.append("--" + profile)
        return self

    """
        Enable force authoring.
    """
    def with_force_authoring(self):
        logging.debug("enable force authoring")
        self.exec_arguments.append("--force-authoring")
        return self

    """
        Specify chain specification.
    """
    def with_chain_spec(self, spec = None, dev = False):
        if dev is True:
            logging.debug("chain spec: `--chain=dev`")
            self.exec_arguments.append("--chain")
            self.exec_arguments.append("dev")
            return self
        elif spec is not None and dev is False:
            logging.debug("chain spec: `%s`" % (spec))

            if os.path.exists(spec) is False:
                raise InvalidConfiguration("`%s` does not exist in the filesystem" % (spec))
            self.exec_arguments.append("--chain")
            self.exec_arguments.append(spec)
            return self
        else:
            raise InvalidConfiguration("must provide either path to file or `dev = True`")

    def build(self):
        logging.info("launch node: path {}, arguments {}".format(self.path, self.exec_arguments))

        self.logfile = open(
            "/tmp/%s-%d-%d" % (
                self.type_registry_preset,
                self.p2p_port,
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

        # TODO: zzz
        time.sleep(1)

        self.substrate = SubstrateInterface(
            url = "ws://127.0.0.1:%d" % (self.ws_port),
            type_registry_preset = self.type_registry_preset,
        )

        # get local peer id
        result = self.substrate.rpc_request("system_localPeerId", params = None).get("result")
        if result is not None:
            logging.debug("local peer id: %s" % (result))
            self.local_peer_id = result
        else:
            raise RpcQueryError("failed to fetch local peer id: `%s`" % (response['error']['message']))

        # fetch metadata for filters
        response = self.substrate.rpc_request("state_getMetadata", params = None)

        if 'error' in response:
            raise RpcQueryError("failed to fetch metadata: `%s`" % (response['error']['message']))

        if response.get('result'):
            RuntimeConfiguration().update_type_registry(load_type_registry_preset(name="core"))
            self.metadata = RuntimeConfiguration().create_scale_object(
                'MetadataVersioned', data=ScaleBytes(response.get('result'))
            )
            self.metadata.decode()

        return self

    def __del__(self):
        self.process.terminate()

    """
        Get chain metadata.
    """
    def get_metadata(self):
        self.metadata

    """
        Get local peer ID.
    """
    def get_local_peer_id(self):
        return self.local_peer_id

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
        super().__init__(
            type_registry_preset = "substrate-node-template",
            default_path = "/usr/local/bin/node-template"
        )
