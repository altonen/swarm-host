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
    }
}

class PeerHeuristics {
    constructor() {
        this.protocols = new Map();
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

        headerRow.appendChild(headerCell1);
        headerRow.appendChild(headerCell2);
        headerRow.appendChild(headerCell3);
        headerRow.appendChild(headerCell4);
        headerRow.appendChild(headerCell5);
        table.appendChild(headerRow);

        value["protocols"].forEach((data, protocol) => {
            let row = document.createElement("tr");

            let cell1 = document.createElement("td");
            cell1.textContent = protocol;
            cell1.style.textAlign = "center";

            let cell2 = document.createElement("td");
            cell2.textContent = data.total_bytes_sent;
            cell2.style.textAlign = "center";

            let cell3 = document.createElement("td");
            cell3.textContent = data.total_bytes_received;
            cell3.style.textAlign = "center";

            let cell4 = document.createElement("td");
            cell4.textContent = data.redundant_bytes_sent;
            cell4.style.textAlign = "center";

            let cell5 = document.createElement("td");
            cell5.textContent = data.redundant_bytes_received;
            cell5.style.textAlign = "center";

            row.appendChild(cell1);
            row.appendChild(cell2);
            row.appendChild(cell3);
            row.appendChild(cell4);
            row.appendChild(cell5);
            table.appendChild(row);
        });

        document.getElementById("heuristics").appendChild(peer_id);
        document.getElementById("heuristics").appendChild(table);
    });
});

