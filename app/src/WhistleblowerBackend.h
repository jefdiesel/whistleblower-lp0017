// SPDX-License-Identifier: MIT OR Apache-2.0
//
// WhistleblowerBackend — C++/Qt6 helper exposed to the QML UI.
//
// The QML engine runs in a network-sandboxed environment and cannot do IO; this
// QObject only performs pure, local computation (hashing + JSON assembly + a
// clock read). All network/storage/chain IO is done from QML via the injected
// `logos` bridge against the storage_module, delivery_module and
// whistleblower_registry core modules.
//
// IMPORTANT — canonical hashing:
//   metadataHash() reimplements, byte-for-byte, the canonical algorithm in
//   crates/wb-types/src/hash.rs (`canonical_metadata_bytes` + SHA-256). The two
//   implementations MUST stay byte-identical: the registry stores this hash and
//   the Rust batch-anchor tool recomputes it from the broadcast envelope, so any
//   divergence makes documents un-anchorable / unverifiable. If you ever change
//   one, change the other and re-check against the shared test vector noted
//   below.

#ifndef WHISTLEBLOWER_BACKEND_H
#define WHISTLEBLOWER_BACKEND_H

#include <QObject>
#include <QString>
#include <QStringList>
#if __has_include(<QtQml/qqmlregistration.h>)
#include <QtQml/qqmlregistration.h>
#define WB_HAS_QML_REGISTRATION 1
#endif

class WhistleblowerBackend : public QObject {
    Q_OBJECT
    // Auto-register with the QML engine when built as part of a qt_add_qml_module
    // (URI "Whistleblower"), so QML can do `import Whistleblower 1.0` and
    // instantiate `WhistleblowerBackend {}`. Equivalent to a manual
    // qmlRegisterType<WhistleblowerBackend>("Whistleblower", 1, 0,
    // "WhistleblowerBackend"). Guarded so the class still compiles in a plain
    // (non-QML) translation unit.
#ifdef WB_HAS_QML_REGISTRATION
    QML_ELEMENT
#endif

public:
    explicit WhistleblowerBackend(QObject *parent = nullptr);

    // Canonical 32-byte SHA-256 metadata hash, returned as lowercase hex.
    //
    // Mirrors crates/wb-types/src/hash.rs::metadata_hash. The preimage is:
    //   LP("WB-META-v1") || LP(cid) || LP(title) || LP(description)
    //     || LP(content_type) || u64_le(size_bytes) || u64_le(timestamp)
    //     || u32_le(N) || LP(tag_0) .. LP(tag_{N-1})
    // where tags are sorted + de-duplicated first and
    //   LP(s) = u32_le(utf8_byte_len(s)) || utf8_bytes(s).
    Q_INVOKABLE QString metadataHash(QString cid,
                                     QString title,
                                     QString description,
                                     QString contentType,
                                     qlonglong sizeBytes,
                                     qlonglong timestamp,
                                     QStringList tags) const;

    // Build the JSON Delivery payload (compact) matching wb_types::MetadataEnvelope:
    // {cid,title,description,content_type,size_bytes,timestamp,tags,schema_version:1}
    Q_INVOKABLE QString buildEnvelopeJson(QString cid,
                                          QString title,
                                          QString description,
                                          QString contentType,
                                          qlonglong sizeBytes,
                                          qlonglong timestamp,
                                          QStringList tags) const;

    // Current Unix time in milliseconds (matches MetadataEnvelope.timestamp unit).
    Q_INVOKABLE qlonglong nowMs() const;
};

#endif // WHISTLEBLOWER_BACKEND_H
