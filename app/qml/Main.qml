// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Whistleblower (LP-0017) — censorship-resistant document upload UI.
//
// The QML engine is network-sandboxed: it performs NO direct IO. Everything goes
// through Logos Core modules via the injected `logos` bridge:
//   logos.callModuleAsync(module, method, [args], callback, timeoutMs=30000)
//   logos.callModule(module, method, [args]) -> JSON string   (sync)
//   logos.onModuleEvent(module, eventName)   -> subscribe to a module signal
//   Connections { target: logos; function onModuleEventReceived(name, event, dataJson){} }
//
// Pure local computation (canonical hashing, envelope JSON, clock) is delegated
// to the C++ WhistleblowerBackend, registered as the QML type
// `Whistleblower.WhistleblowerBackend` (see app/CMakeLists.txt).

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import Whistleblower 1.0

ApplicationWindow {
    id: root
    visible: true
    width: 900
    height: 720
    title: qsTr("Whistleblower")

    // Canonical Delivery content topic (see crates/wb-types/src/topic.rs).
    readonly property string topic: "/whistleblower/1/documents/json"

    // delivery_module createNode config (verified reasonable default).
    readonly property string nodeConfigJson:
        '{"logLevel":"INFO","mode":"Core","preset":"logos.dev"}'

    // --- mutable UI state -----------------------------------------------------
    property string filePath: ""        // local path of the chosen file (from FileDialog)
    property string cid: ""             // CID returned by storage_module after upload
    property string contentType: ""     // best-effort MIME guess from the extension
    property int    sizeBytes: 0        // file size (not always available pre-upload)
    property bool   busy: false         // an upload/broadcast is in flight
    property bool   nodeReady: false    // delivery node started + subscribed

    // Pure-computation helper implemented in C++.
    WhistleblowerBackend { id: backend }

    // Received documents (populated from delivery_module messageReceived events).
    ListModel { id: documentsModel }

    // ------------------------------------------------------------------ helpers
    function log(msg) {
        const ts = Qt.formatDateTime(new Date(), "hh:mm:ss");
        statusArea.text += "[" + ts + "] " + msg + "\n";
        // Auto-scroll to the latest line.
        statusArea.cursorPosition = statusArea.length;
    }

    // Split the comma-separated tags field into a trimmed, non-empty string list.
    function parsedTags() {
        const raw = tagsField.text.split(",");
        const out = [];
        for (let i = 0; i < raw.length; ++i) {
            const t = raw[i].trim();
            if (t.length > 0)
                out.push(t);
        }
        return out;
    }

    // Very small extension -> MIME map; the registry only needs *a* content_type,
    // and the Rust side treats it as opaque. Defaults to octet-stream.
    function guessContentType(path) {
        const lower = path.toLowerCase();
        if (lower.endsWith(".pdf"))  return "application/pdf";
        if (lower.endsWith(".txt"))  return "text/plain";
        if (lower.endsWith(".md"))   return "text/markdown";
        if (lower.endsWith(".json")) return "application/json";
        if (lower.endsWith(".png"))  return "image/png";
        if (lower.endsWith(".jpg") || lower.endsWith(".jpeg")) return "image/jpeg";
        if (lower.endsWith(".zip"))  return "application/zip";
        if (lower.endsWith(".csv"))  return "text/csv";
        if (lower.endsWith(".mp4"))  return "video/mp4";
        return "application/octet-stream";
    }

    // Strip a file:// URL down to a local filesystem path for storage_module.
    function urlToLocalPath(url) {
        const s = url.toString();
        if (s.startsWith("file://"))
            return decodeURIComponent(s.replace(/^file:\/\//, ""));
        return decodeURIComponent(s);
    }

    // --------------------------------------------------------- module bootstrap
    Component.onCompleted: {
        // Subscribe to the events we care about BEFORE starting the node so we
        // don't miss early signals.
        logos.onModuleEvent("delivery_module", "messageReceived");
        logos.onModuleEvent("delivery_module", "nodeStarted");
        logos.onModuleEvent("delivery_module", "connectionStateChanged");
        logos.onModuleEvent("storage_module", "storageUploadDone");

        log("Creating delivery node...");
        logos.callModuleAsync("delivery_module", "createNode", [nodeConfigJson],
            function (resp) {
                log("createNode -> " + resp);
                logos.callModuleAsync("delivery_module", "start", [],
                    function (startResp) {
                        log("start -> " + startResp);
                        // Some builds emit nodeStarted async; subscribe now anyway,
                        // and (idempotently) again on nodeStarted below.
                        subscribeTopic();
                    });
            });
    }

    function subscribeTopic() {
        logos.callModuleAsync("delivery_module", "subscribe", [topic],
            function (resp) {
                nodeReady = true;
                log("subscribed " + topic + " -> " + resp);
            });
    }

    // ----------------------------------------------------------- bridge signals
    Connections {
        target: logos
        function onModuleEventReceived(name, event, dataJsonString) {
            if (name === "delivery_module" && event === "nodeStarted") {
                log("delivery node started");
                subscribeTopic();
                return;
            }
            if (name === "delivery_module" && event === "connectionStateChanged") {
                log("delivery connection: " + dataJsonString);
                return;
            }
            if (name === "delivery_module" && event === "messageReceived") {
                handleMessageReceived(dataJsonString);
                return;
            }
            if (name === "storage_module" && event === "storageUploadDone") {
                handleUploadDone(dataJsonString);
                return;
            }
        }
    }

    // messageReceived(messageHash, contentTopic, payload, timestamp) is delivered
    // as a JSON object. The payload is our MetadataEnvelope JSON; parse it and add
    // the document to the list — demonstrating "immediately findable via Delivery".
    function handleMessageReceived(dataJsonString) {
        let data;
        try {
            data = JSON.parse(dataJsonString);
        } catch (e) {
            log("messageReceived: bad event JSON: " + e);
            return;
        }
        // Only react to our topic if the event carries one.
        if (data.contentTopic !== undefined && data.contentTopic !== topic)
            return;

        // payload may already be a string of JSON; tolerate either shape.
        let payloadStr = data.payload;
        if (typeof payloadStr !== "string")
            payloadStr = JSON.stringify(payloadStr);

        let env;
        try {
            env = JSON.parse(payloadStr);
        } catch (e) {
            log("received non-envelope payload, ignoring");
            return;
        }
        if (!env || !env.cid)
            return;

        documentsModel.insert(0, {
            "docTitle": env.title !== undefined ? env.title : "(untitled)",
            "docCid": env.cid,
            "docContentType": env.content_type !== undefined ? env.content_type : ""
        });
        log("received document: " + (env.title || env.cid));
    }

    // storageUploadDone -> {success, sessionId, cid}. On success, broadcast the
    // metadata envelope over Delivery and enable on-chain anchoring.
    function handleUploadDone(dataJsonString) {
        let data;
        try {
            data = JSON.parse(dataJsonString);
        } catch (e) {
            busy = false;
            log("storageUploadDone: bad event JSON: " + e);
            return;
        }
        if (!data.success) {
            busy = false;
            log("upload failed: " + dataJsonString);
            return;
        }

        root.cid = data.cid;
        cidField.text = data.cid;
        log("upload done, cid=" + data.cid);

        // Build the canonical Delivery payload and broadcast it.
        const ts = backend.nowMs();
        const envelopeJson = backend.buildEnvelopeJson(
            root.cid,
            titleField.text,
            descriptionField.text,
            root.contentType,
            root.sizeBytes,
            ts,
            parsedTags());

        // Remember the timestamp/tags used so the on-chain hash matches the
        // broadcast envelope exactly.
        root._lastTimestamp = ts;
        root._lastTags = parsedTags();

        log("broadcasting envelope to " + topic);
        logos.callModuleAsync("delivery_module", "send", [topic, envelopeJson],
            function (resp) {
                busy = false;
                log("send -> " + resp);
                log("Document is now discoverable via Delivery. " +
                    "Use 'Anchor on-chain' to optionally register it.");
            });
    }

    // The timestamp/tags actually broadcast, reused for anchoring so the
    // metadata_hash is computed over identical inputs.
    property double _lastTimestamp: 0
    property var _lastTags: []

    // ---------------------------------------------------------------- file pick
    FileDialog {
        id: fileDialog
        title: qsTr("Choose a document to upload")
        onAccepted: {
            root.filePath = urlToLocalPath(selectedFile);
            root.contentType = guessContentType(root.filePath);
            // Reset any prior CID — a new file means a new upload.
            root.cid = "";
            cidField.text = "";
            filePathField.text = root.filePath;
            log("selected " + root.filePath + " (" + root.contentType + ")");
        }
    }

    // ----------------------------------------------------------------- layout
    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            width: root.width
            spacing: 12
            anchors.margins: 16
            anchors.left: parent.left
            anchors.right: parent.right

            Label {
                text: qsTr("Whistleblower — upload, broadcast, and (optionally) anchor a document")
                font.pixelSize: 18
                font.bold: true
                Layout.fillWidth: true
                Layout.topMargin: 12
                Layout.leftMargin: 16
                Layout.rightMargin: 16
            }

            // ---- File picker + path/CID -----------------------------------
            GroupBox {
                title: qsTr("Document")
                Layout.fillWidth: true
                Layout.leftMargin: 16
                Layout.rightMargin: 16

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 8

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        Button {
                            text: qsTr("Choose file…")
                            onClicked: fileDialog.open()
                        }
                        TextField {
                            id: filePathField
                            Layout.fillWidth: true
                            readOnly: true
                            placeholderText: qsTr("No file selected")
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        Label { text: qsTr("CID:") }
                        TextField {
                            id: cidField
                            Layout.fillWidth: true
                            readOnly: true
                            placeholderText: qsTr("(assigned by storage after upload)")
                        }
                    }
                }
            }

            // ---- Metadata -------------------------------------------------
            GroupBox {
                title: qsTr("Metadata")
                Layout.fillWidth: true
                Layout.leftMargin: 16
                Layout.rightMargin: 16

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 8

                    Label { text: qsTr("Title") }
                    TextField {
                        id: titleField
                        Layout.fillWidth: true
                        placeholderText: qsTr("Short human title")
                    }

                    Label { text: qsTr("Description") }
                    TextArea {
                        id: descriptionField
                        Layout.fillWidth: true
                        Layout.preferredHeight: 80
                        wrapMode: TextEdit.Wrap
                        placeholderText: qsTr("What is this document?")
                    }

                    Label { text: qsTr("Tags (comma-separated)") }
                    TextField {
                        id: tagsField
                        Layout.fillWidth: true
                        placeholderText: qsTr("leak, memo, finance")
                    }
                }
            }

            // ---- Actions --------------------------------------------------
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 16
                Layout.rightMargin: 16
                spacing: 12

                Button {
                    id: uploadButton
                    text: busy ? qsTr("Working…") : qsTr("Upload & Broadcast")
                    enabled: !busy && root.filePath !== "" && titleField.text.trim() !== ""
                    onClicked: doUploadAndBroadcast()
                }

                // Distinct, separate action (LP-0017 requires anchoring to be an
                // optional step distinct from upload). Enabled only once a CID
                // exists for the currently-broadcast document.
                Button {
                    id: anchorButton
                    text: qsTr("Anchor on-chain")
                    enabled: !busy && root.cid !== ""
                    onClicked: doAnchor()
                }

                Item { Layout.fillWidth: true } // spacer

                BusyIndicator {
                    running: busy
                    visible: busy
                    implicitWidth: 24
                    implicitHeight: 24
                }
            }

            // ---- Status / log --------------------------------------------
            GroupBox {
                title: qsTr("Status")
                Layout.fillWidth: true
                Layout.leftMargin: 16
                Layout.rightMargin: 16

                ScrollView {
                    anchors.fill: parent
                    implicitHeight: 140
                    TextArea {
                        id: statusArea
                        readOnly: true
                        wrapMode: TextEdit.Wrap
                        font.family: "monospace"
                        placeholderText: qsTr("Activity log…")
                    }
                }
            }

            // ---- Received documents --------------------------------------
            GroupBox {
                title: qsTr("Documents seen on the network")
                Layout.fillWidth: true
                Layout.leftMargin: 16
                Layout.rightMargin: 16
                Layout.bottomMargin: 16

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 4

                    Label {
                        visible: documentsModel.count === 0
                        text: qsTr("No documents received yet. Broadcasts on %1 will appear here.").arg(root.topic)
                        opacity: 0.7
                    }

                    ListView {
                        id: documentsView
                        Layout.fillWidth: true
                        Layout.preferredHeight: 180
                        clip: true
                        model: documentsModel
                        spacing: 6
                        ScrollBar.vertical: ScrollBar {}

                        delegate: Rectangle {
                            width: ListView.view ? ListView.view.width : 0
                            height: itemCol.implicitHeight + 12
                            radius: 6
                            border.width: 1
                            border.color: "#cccccc"
                            color: "transparent"

                            ColumnLayout {
                                id: itemCol
                                x: 8
                                y: 6
                                width: parent.width - 16
                                spacing: 2
                                Label {
                                    text: model.docTitle
                                    font.bold: true
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                                Label {
                                    text: qsTr("CID: ") + model.docCid
                                    font.family: "monospace"
                                    font.pixelSize: 11
                                    elide: Text.ElideMiddle
                                    Layout.fillWidth: true
                                }
                                Label {
                                    text: model.docContentType
                                    font.pixelSize: 11
                                    opacity: 0.7
                                    visible: model.docContentType !== ""
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // -------------------------------------------------------------- actions impl
    function doUploadAndBroadcast() {
        if (root.filePath === "") {
            log("no file selected");
            return;
        }
        busy = true;
        log("uploading " + root.filePath + " ...");
        // chunkSize 65536 (64 KiB) is a reasonable default for storage_module.
        logos.callModuleAsync("storage_module", "uploadUrl", [root.filePath, 65536],
            function (resp) {
                // The terminal result arrives via the storageUploadDone event;
                // this callback just acknowledges the request was accepted.
                log("uploadUrl accepted -> " + resp);
            });
    }

    function doAnchor() {
        if (root.cid === "") {
            log("nothing to anchor (no CID)");
            return;
        }
        // Compute the canonical hash over the SAME inputs that were broadcast so
        // it matches what the batch-anchor tool would derive from the envelope.
        const ts = root._lastTimestamp > 0 ? root._lastTimestamp : backend.nowMs();
        const tags = root._lastTags && root._lastTags.length !== undefined
            ? root._lastTags : parsedTags();

        const hashHex = backend.metadataHash(
            root.cid,
            titleField.text,
            descriptionField.text,
            root.contentType,
            root.sizeBytes,
            ts,
            tags);

        log("anchoring cid=" + root.cid + " metadata_hash=" + hashHex);

        // NOTE: exact argument marshalling for the generated whistleblower_registry
        // module may need tweaking on the provisioned machine. The Rust seam
        // (crates/wb-index/src/registry.rs::anchor_batch) takes a slice of
        // {cid, metadata_hash} entries plus an anchor_timestamp_ms. We pass a
        // JSON-array-of-objects + a millisecond timestamp; the generated
        // logos-module wrapper may instead expect borsh/hex-encoded args, a flat
        // arg list, or string-typed numbers — adjust here to match the IDL.
        const entries = [ { "cid": root.cid, "metadata_hash": hashHex } ];
        logos.callModuleAsync("whistleblower_registry", "anchorBatch",
            [ entries, backend.nowMs() ],
            function (resp) {
                log("anchorBatch -> " + resp);
            });
    }
}
