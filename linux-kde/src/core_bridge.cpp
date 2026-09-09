#include "core_bridge.h"

#include <QJsonArray>
#include <QJsonValue>

extern "C" {
// Stable C ABI from rust/lithe-core/src/runtime/ffi.rs. Returned strings are
// owned by the caller and must be released with lithe_core_free_string.
char *lithe_core_execute_json(const char *request);
void lithe_core_free_string(char *value);
}

namespace {

CoreBridge::Result executeJson(const QString &command, const QJsonObject &payload) {
    const QJsonObject request{
        {"id", QString::number(QDateTime::currentMSecsSinceEpoch())},
        {"operationId", QString::number(QDateTime::currentMSecsSinceEpoch())},
        {"command", command},
        {"payload", payload},
    };
    const QByteArray requestBytes =
        QJsonDocument(request).toJson(QJsonDocument::Compact);

    char *raw = lithe_core_execute_json(requestBytes.constData());
    if (raw == nullptr) {
        return {.errorMessage = QStringLiteral("Shared core returned a null pointer"),
                .ok = false};
    }
    const QByteArray responseBytes(raw);
    lithe_core_free_string(raw);

    QJsonParseError parseError{};
    const QJsonDocument envelope =
        QJsonDocument::fromJson(responseBytes, &parseError);
    if (parseError.error != QJsonParseError::NoError) {
        return {.errorMessage = QStringLiteral("Shared core returned invalid JSON: %1")
                                    .arg(parseError.errorString()),
                .ok = false};
    }

    const QJsonObject root = envelope.object();
    CoreBridge::Result result;
    if (root.value("ok").toBool()) {
        result.ok = true;
        result.value = QJsonDocument(root.value("data").toObject())
                           .toJson(QJsonDocument::Compact);
    } else {
        result.errorMessage =
            root.value("error").toObject().value("message").toString(
                QStringLiteral("Shared core operation failed"));
    }
    return result;
}

}  // namespace

CoreBridge::Result CoreBridge::execute(const QString &command,
                                       const QJsonObject &payload) {
    return executeJson(command, payload);
}

QStringList CoreBridge::workspaceFiles(const QString &root) {
    QStringList files;
    const Result result =
        execute(QStringLiteral("workspace.snapshot"),
                QJsonObject{{"root", root}});
    if (!result.ok) {
        return files;
    }
    const QJsonDocument document = QJsonDocument::fromJson(result.value.toUtf8());
    for (const QJsonValue &entry : document.object().value("files").toArray()) {
        files.append(entry.toString());
    }
    return files;
}

CoreBridge::Result CoreBridge::readFile(const QString &root, const QString &path) {
    // The `data` payload carries a `text` field; surface exactly that so the
    // editor receives file contents directly.
    Result result = execute(QStringLiteral("file.read"),
                            QJsonObject{{"root", root}, {"path", path}});
    if (result.ok) {
        const QString text = QJsonDocument::fromJson(result.value.toUtf8())
                                 .object()
                                 .value("text")
                                 .toString();
        result.value = text;
    }
    return result;
}

bool CoreBridge::writeFile(const QString &root, const QString &path,
                           const QString &text, QString *errorMessage) {
    const Result result = execute(
        QStringLiteral("file.write"),
        QJsonObject{{"root", root}, {"path", path}, {"text", text}});
    if (!result.ok && errorMessage != nullptr) {
        *errorMessage = result.errorMessage;
    }
    return result.ok;
}

std::optional<QString> CoreBridge::gitBranch(const QString &root) {
    const Result result =
        execute(QStringLiteral("git.status"), QJsonObject{{"root", root}});
    if (!result.ok) {
        return std::nullopt;
    }
    const QString branch = QJsonDocument::fromJson(result.value.toUtf8())
                               .object()
                               .value("branch")
                               .toString();
    if (branch.isEmpty()) {
        return std::nullopt;
    }
    return branch;
}
