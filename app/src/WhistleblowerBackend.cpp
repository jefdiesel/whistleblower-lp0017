// SPDX-License-Identifier: MIT OR Apache-2.0
//
// See WhistleblowerBackend.h. The hashing here is a line-by-line port of
// crates/wb-types/src/hash.rs and MUST remain byte-identical to it.
//
// Shared test vector (from crates/wb-types/src/hash.rs tests):
//   cid          = "zDvSampleCidAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
//   title        = "Leaked memo"
//   description  = "Internal memo describing X"
//   content_type = "application/pdf"
//   size_bytes   = 12345
//   timestamp    = 1700000000000
//   tags         = ["leak","memo"]   (any order / with dupes -> same hash)
// Use this exact input to cross-check metadataHash() against the Rust
// implementation when validating on the provisioned machine.

#include "WhistleblowerBackend.h"

#include <QByteArray>
#include <QCryptographicHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QDateTime>

#include <algorithm>
#include <cstdint>

namespace {

// Append a little-endian u32 (mirrors `(x as u32).to_le_bytes()` in Rust).
void putU32Le(QByteArray &buf, quint32 v) {
    buf.append(static_cast<char>(v & 0xFF));
    buf.append(static_cast<char>((v >> 8) & 0xFF));
    buf.append(static_cast<char>((v >> 16) & 0xFF));
    buf.append(static_cast<char>((v >> 24) & 0xFF));
}

// Append a little-endian u64 (mirrors `(x as u64).to_le_bytes()` in Rust).
void putU64Le(QByteArray &buf, quint64 v) {
    for (int i = 0; i < 8; ++i) {
        buf.append(static_cast<char>((v >> (8 * i)) & 0xFF));
    }
}

// LP(s) = u32_le(byte_len(utf8)) || utf8_bytes(s).
// NOTE: length is the UTF-8 *byte* length, matching Rust `String::len()`.
void putLengthPrefixed(QByteArray &buf, const QByteArray &utf8) {
    putU32Le(buf, static_cast<quint32>(utf8.size()));
    buf.append(utf8);
}

void putLengthPrefixed(QByteArray &buf, const QString &s) {
    putLengthPrefixed(buf, s.toUtf8());
}

// Sort + de-duplicate tags using *byte-wise* (UTF-8 lexicographic) ordering so
// the result matches Rust's `tags.sort_unstable(); tags.dedup();` on `&str`,
// which compares by UTF-8 bytes. (QString's default operator< is UTF-16/locale
// aware and would diverge for some non-ASCII inputs — so we sort on toUtf8().)
QList<QByteArray> normalizedTagBytes(const QStringList &tags) {
    QList<QByteArray> out;
    out.reserve(tags.size());
    for (const QString &t : tags) {
        out.append(t.toUtf8());
    }
    std::sort(out.begin(), out.end(), [](const QByteArray &a, const QByteArray &b) {
        // Compare as unsigned bytes, like Rust's slice/str Ord.
        const int n = std::min(a.size(), b.size());
        for (int i = 0; i < n; ++i) {
            const unsigned char ca = static_cast<unsigned char>(a[i]);
            const unsigned char cb = static_cast<unsigned char>(b[i]);
            if (ca != cb) {
                return ca < cb;
            }
        }
        return a.size() < b.size();
    });
    out.erase(std::unique(out.begin(), out.end()), out.end());
    return out;
}

} // namespace

WhistleblowerBackend::WhistleblowerBackend(QObject *parent) : QObject(parent) {}

QString WhistleblowerBackend::metadataHash(QString cid,
                                           QString title,
                                           QString description,
                                           QString contentType,
                                           qlonglong sizeBytes,
                                           qlonglong timestamp,
                                           QStringList tags) const {
    // Build the canonical preimage (see crates/wb-types/src/hash.rs).
    QByteArray buf;

    putLengthPrefixed(buf, QStringLiteral("WB-META-v1")); // domain separator
    putLengthPrefixed(buf, cid);
    putLengthPrefixed(buf, title);
    putLengthPrefixed(buf, description);
    putLengthPrefixed(buf, contentType);
    putU64Le(buf, static_cast<quint64>(sizeBytes));
    putU64Le(buf, static_cast<quint64>(timestamp));

    const QList<QByteArray> sortedTags = normalizedTagBytes(tags);
    putU32Le(buf, static_cast<quint32>(sortedTags.size()));
    for (const QByteArray &t : sortedTags) {
        putLengthPrefixed(buf, t);
    }

    const QByteArray digest = QCryptographicHash::hash(buf, QCryptographicHash::Sha256);
    return QString::fromLatin1(digest.toHex()); // lowercase hex, matches hex::encode
}

QString WhistleblowerBackend::buildEnvelopeJson(QString cid,
                                                QString title,
                                                QString description,
                                                QString contentType,
                                                qlonglong sizeBytes,
                                                qlonglong timestamp,
                                                QStringList tags) const {
    // Field names match wb_types::MetadataEnvelope serde output. Tags are emitted
    // as-provided here (the canonical hash is what normalizes order/dupes); the
    // Rust side accepts any order and re-normalizes when hashing.
    QJsonObject obj;
    obj.insert(QStringLiteral("cid"), cid);
    obj.insert(QStringLiteral("title"), title);
    obj.insert(QStringLiteral("description"), description);
    obj.insert(QStringLiteral("content_type"), contentType);
    // JSON numbers are doubles; CIDs/sizes/timestamps here fit safely. Stored as
    // qint64 to keep integer formatting (no scientific notation) for these ranges.
    obj.insert(QStringLiteral("size_bytes"), static_cast<qint64>(sizeBytes));
    obj.insert(QStringLiteral("timestamp"), static_cast<qint64>(timestamp));

    QJsonArray tagArr;
    for (const QString &t : tags) {
        tagArr.append(t);
    }
    obj.insert(QStringLiteral("tags"), tagArr);
    obj.insert(QStringLiteral("schema_version"), 1);

    return QString::fromUtf8(QJsonDocument(obj).toJson(QJsonDocument::Compact));
}

qlonglong WhistleblowerBackend::nowMs() const {
    return static_cast<qlonglong>(QDateTime::currentMSecsSinceEpoch());
}
