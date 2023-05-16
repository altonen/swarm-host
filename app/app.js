class MessageHeuristics {
    constructor() {
        this.total_bytes_sent = 0;
        this.total_messages_sent = 0;
        this.total_bytes_received = 0;
        this.total_messages_received = 0;
        this.redundant_bytes_sent = 0;
        this.redundant_bytes_received = 0;
        this.unique_messages_sent = new Set();
        this.unique_messages_received = new Set();
        this.interfaces = new Set();
    }
}

class PeerHeuristics {
    constructor() {
        this.protocols = new Map();
        this.interfaces = new Set();
    }

    addProtocol(protocol, messageHeuristics) {
        this.protocols.set(protocol, messageHeuristics);
    }

    getProtocol(protocol) {
        return this.protocols.get(protocol);
    }
}

const socket = new WebSocket('ws://localhost:8885');
const peerHeuristicsMap = new Map();

socket.addEventListener('message', (event) => {
    let peers = JSON.parse(event.data);

    for (var peer in peers) {
        var heuristics = peerHeuristicsMap.get(peer);

        if (!heuristics) {
            heuristics = new PeerHeuristics();
            peerHeuristicsMap.set(peer, heuristics);
        }

        for (var protocol in peers[peer]["protocols"]) {
            var protocolHeuristics = heuristics.protocols.get(protocol);

            if (!protocolHeuristics) {
                protocolHeuristics = new MessageHeuristics();
                heuristics.protocols.set(protocol, protocolHeuristics);
            }

            protocolHeuristics.total_bytes_sent += peers[peer]["protocols"][protocol].total_bytes_sent;
            protocolHeuristics.total_bytes_sent += peers[peer]["protocols"][protocol].total_bytes_sent;
            protocolHeuristics.total_messages_sent += peers[peer]["protocols"][protocol].total_messages_sent;
            protocolHeuristics.total_bytes_received += peers[peer]["protocols"][protocol].total_bytes_received;
            protocolHeuristics.total_messages_received += peers[peer]["protocols"][protocol].total_messages_received;
            protocolHeuristics.redundant_bytes_sent += peers[peer]["protocols"][protocol].redundant_bytes_sent;
            protocolHeuristics.redundant_bytes_received += peers[peer]["protocols"][protocol].redundant_bytes_received;
            protocolHeuristics.interfaces = peers[peer]["protocols"][protocol].interfaces;
        }

        for (var interface in peers[peer]['interfaces']) {
            heuristics.interfaces.add(interface);
        }
    }

    document.getElementById("heuristics").innerHTML = "";

    // create table of peers and append relevant data to the table
    peerHeuristicsMap.forEach((value, key) => {
        let peer_id = document.createElement("h4");
        peer_id.textContent = key;

        let table = document.createElement("table");
        let headerRow = document.createElement("tr");

        let headerCell1 = document.createElement("th");
        headerCell1.textContent = "Protocol";

        let headerCell2 = document.createElement("th");
        headerCell2.textContent = "Total sent";

        let headerCell3 = document.createElement("th");
        headerCell3.textContent = "Total received";

        let headerCell4 = document.createElement("th");
        headerCell4.textContent = "Redundant sent";

        let headerCell5 = document.createElement("th");
        headerCell5.textContent = "Redundant received";

        let headerCell6 = document.createElement("th");
        headerCell6.textContent = "Interfaces";

        headerRow.appendChild(headerCell1);
        headerRow.appendChild(headerCell2);
        headerRow.appendChild(headerCell3);
        headerRow.appendChild(headerCell4);
        headerRow.appendChild(headerCell5);
        headerRow.appendChild(headerCell6);
        table.appendChild(headerRow);

        value["protocols"].forEach((data, protocol) => {
            let row = document.createElement("tr");

            let cell1 = document.createElement("td");
            cell1.textContent = protocol;
            cell1.style.textAlign = "center";

            let cell2 = document.createElement("td");
            if (data.total_bytes_sent > 1000 * 1000) {
                cell2.textContent = ((data.total_bytes_sent / 1000 / 1000).toFixed(2)) + "MB";
            } else {
                cell2.textContent = ((data.total_bytes_sent / 1000).toFixed(2)) + "KB";
            }
            cell2.style.textAlign = "center";

            let cell3 = document.createElement("td");
            if (data.total_bytes_received > 1000 * 1000) {
                cell3.textContent = ((data.total_bytes_received / 1000 / 1000).toFixed(2)) + "MB";
            } else {
                cell3.textContent = ((data.total_bytes_received / 1000).toFixed(2)) + "KB";
            }
            cell3.style.textAlign = "center";

            let cell4 = document.createElement("td");
            if (data.redundant_bytes_sent > 1000 * 1000) {
                cell4.textContent = ((data.redundant_bytes_sent / 1000 / 1000).toFixed(2)) + "MB";
            } else {
                cell4.textContent = ((data.redundant_bytes_sent / 1000).toFixed(2)) + "KB";
            }
            cell4.style.textAlign = "center";

            let cell5 = document.createElement("td");
            if (data.redundant_bytes_received > 1000 * 1000) {
                cell5.textContent = ((data.redundant_bytes_received / 1000 / 1000).toFixed(2)) + "MB";
            } else {
                cell5.textContent = ((data.redundant_bytes_received / 1000).toFixed(2)) + "KB";
            }
            cell5.style.textAlign = "center";

            let cell6 = document.createElement("td");
            for (var interface in data.interfaces) {
                cell6.textContent = interface + ", ";
            }
            cell6.style.textAlign = "center";

            row.appendChild(cell1);
            row.appendChild(cell2);
            row.appendChild(cell3);
            row.appendChild(cell4);
            row.appendChild(cell5);
            row.appendChild(cell6);
            table.appendChild(row);
        });

        let interfaces = document.createElement("p");
        value['interfaces'].forEach((value, _key) => {
            interfaces.textContent += value + ", ";
        });

        document.getElementById("heuristics").appendChild(peer_id);
        document.getElementById("heuristics").appendChild(table);
        document.getElementById("heuristics").appendChild(interfaces);
    });
});

