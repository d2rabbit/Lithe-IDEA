#pragma once

#include "core_bridge.h"

#include <QFutureWatcher>
#include <QLabel>
#include <QListWidget>
#include <QMainWindow>
#include <QPlainTextEdit>
#include <QString>
#include <QStringList>

/// KDE/Qt main window: file list + editor over the shared lithe-core commands.
class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget *parent = nullptr);

    /// Opens a workspace at startup (mirrors the Windows host CLI open flow).
    void loadWorkspace(const QString &root);


private slots:
    void pickWorkspace();
    void showAbout();
    void openSelectedFile(QListWidgetItem *item);
    void saveCurrentFile();
    void refreshBranch();

private:
    void setStatus(const QString &message);

    QListWidget *fileList_;
    QPlainTextEdit *editor_;
    QLabel *statusLabel_;
    QLabel *branchLabel_;
    QString workspaceRoot_;
    QString openFilePath_;
    QFutureWatcher<QStringList> *filesWatcher_;
    QFutureWatcher<CoreBridge::Result> *readWatcher_;
    QFutureWatcher<bool> *saveWatcher_;
    QFutureWatcher<std::optional<QString>> *branchWatcher_;
    QStringList pendingFiles_;
};
