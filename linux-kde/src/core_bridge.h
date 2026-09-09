// Shared-core bridge for the KDE/Qt client.
//
// Mirrors the macOS host's consumption model: the Rust `lithe-core` static
// library is reached through the stable C ABI (`lithe_core_execute_json` +
// `lithe_core_free_string`). Returned envelopes are parsed into QJsonDocument
// and only the `data` payload escapes this class.

#pragma once

#include <QJsonDocument>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <optional>

class CoreBridge {
public:
    struct Result {
        QString value;         // `data` payload as raw JSON text on success
        QString errorMessage;  // stable core error message on failure
        bool ok = false;
    };

    /// Executes one shared-core command on the calling thread.
    static Result execute(const QString &command, const QJsonObject &payload);

    /// Lists workspace-relative file paths for one opened workspace root.
    static QStringList workspaceFiles(const QString &root);

    /// Reads one workspace-relative UTF-8 file.
    static Result readFile(const QString &root, const QString &path);

    /// Writes one workspace-relative UTF-8 file.
    static bool writeFile(const QString &root, const QString &path,
                          const QString &text, QString *errorMessage);

    /// Reads the current Git branch for one workspace root, when available.
    static std::optional<QString> gitBranch(const QString &root);
};
