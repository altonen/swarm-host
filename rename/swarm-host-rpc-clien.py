from jsonrpcclient import parse, request
import requests

import subprocess
import time

# connect to remote node
def create_interface(port, address):
    response = requests.post(
        "http://localhost:%d/" % (port),
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

if __name__ == "__main__":
    create_interface(8886, "127.0.0.1:9999")
# # get local peer id
# def get_local_peer_id(self):
#     response = requests.post("http://localhost:%d/" % (self.rpc_port), json=request("get_local_peer_id"))
#     return response.json()["result"]
