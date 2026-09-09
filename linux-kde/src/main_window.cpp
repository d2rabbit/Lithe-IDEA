#include "main_window.h"

#include "core_bridge.h"

#include <QFileDialog>
#include <QFontDatabase>
#include <QFutureWatcher>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QListWidget>
#include <QPlainTextEdit>
#include <QSplitter>
#include <QStatusBar>
#include <QToolBar>
#include <QtConcurrent/QtConcurrentRun>

MainWindow::MainWindow(QWidget *parent) : QMainWindow(parent) {
    setWindowTitle(tr("Lithe"));
    resize(1150, 760);

    fileList_ = new QListWidget(this);
    editor_ = new QPlainTextEdit(this);
    editor_->setFont(QFontDatabase::systemFont(QFontDatabase::FixedFont));
    editor_->setLineWrapMode(QPlainTextEdit::NoWrap);

    auto *splitter = new QSplitter(this);
    splitter->addWidget(fileList_);
    splitter->addWidget(editor_);
    splitter->setStretchFactor(0, 0);
    splitter->setStretchFactor(1, 1);

    statusLabel_ = new QLabel(tr("打开一个项目文件夹开始"), this);
    statusBar()->addWidget(statusLabel_, 1);
    branchLabel_ = new QLabel(this);
    branchLabel_->setToolTip(tr("当前 Git 分支(lithe-core git.status)"));
    statusBar()->addPermanentWidget(branchLabel_);

    auto *toolbar = addToolBar(tr("main"));
    toolbar->setMovable(false);
    QAction *openAction = toolbar->addAction(tr("打开文件夹"));
    connect(openAction, &QAction::triggered, this, &MainWindow::pickWorkspace);

    setCentralWidget(splitter);

    filesWatcher_ = new QFutureWatcher<QStringList>(this);
    readWatcher_ = new QFutureWatcher<CoreBridge::Result>(this);
    saveWatcher_ = new QFutureWatcher<bool>(this);
    branchWatcher_ = new QFutureWatcher<std::optional<QString>>(this);

    connect(fileList_, &QListWidget::itemClicked, this,
            &MainWindow::openSelectedFile);
    connect(filesWatcher_, &QFutureWatcher<QStringList>::finished, this, [this] {
        fileList_->clear();
        pendingFiles_ = filesWatcher_->result();
        fileList_->addItems(pendingFiles_);
    });
    connect(readWatcher_, &QFutureWatcher<CoreBridge::Result>::finished, this,
            [this] {
                const CoreBridge::Result result = readWatcher_->result();
                if (result.ok) {
                    editor_->setPlainText(result.value);
                    setStatus(tr("已打开 %1(Ctrl+S 保存)").arg(openFilePath_));
                } else {
                    setStatus(result.errorMessage);
                }
            });
    connect(saveWatcher_, &QFutureWatcher<bool>::finished, this, [this] {
        if (saveWatcher_->result()) {
            setStatus(tr("已保存 %1").arg(openFilePath_));
        } else {
            setStatus(tr("保存失败"));
        }
    });
    connect(branchWatcher_,
            &QFutureWatcher<std::optional<QString>>::finished, this, [this] {
                const std::optional<QString> branch = branchWatcher_->result();
                branchLabel_->setText(branch ? *branch : QStringLiteral("no git"));
            });

    // Ctrl+S saves the open file; handled before the editor sees the key.
    fileList_->installEventFilter(this);
    editor_->installEventFilter(this);
}

bool MainWindow::eventFilter(QObject *watched, QEvent *event) {
    if (event->type() == QEvent::KeyPress) {
        auto *keyEvent = static_cast<QKeyEvent *>(event);
        if (keyEvent->modifiers() & Qt::ControlModifier &&
            keyEvent->key() == Qt::Key_S && !openFilePath_.isEmpty()) {
            saveCurrentFile();
            return true;
        }
    }
    return QMainWindow::eventFilter(watched, event);
}

void MainWindow::loadWorkspace(const QString &root) {
    workspaceRoot_ = root;
    const QString workspaceName =
        QFileInfo(root).fileName().isEmpty() ? root : QFileInfo(root).fileName();
    setWindowTitle(tr("Lithe — %1").arg(workspaceName));
    setStatus(tr("工作区:%1").arg(root));
    refreshBranch();

    filesWatcher_->setFuture(
        QtConcurrent::run([root]() { return CoreBridge::workspaceFiles(root); }));
}

void MainWindow::pickWorkspace() {
    const QString root = QFileDialog::getExistingDirectory(
        this, tr("打开项目文件夹"), QString());
    if (root.isEmpty()) {
        return;
    }
    loadWorkspace(root);
}

void MainWindow::openSelectedFile(QListWidgetItem *item) {
    if (item == nullptr || workspaceRoot_.isEmpty()) {
        return;
    }
    openFilePath_ = item->text();
    setStatus(tr("读取 %1 …").arg(openFilePath_));
    const QString root = workspaceRoot_;
    const QString path = openFilePath_;
    readWatcher_->setFuture(QtConcurrent::run(
        [root, path]() { return CoreBridge::readFile(root, path); }));
}

void MainWindow::saveCurrentFile() {
    if (workspaceRoot_.isEmpty() || openFilePath_.isEmpty()) {
        setStatus(tr("没有已打开的文件"));
        return;
    }
    const QString root = workspaceRoot_;
    const QString path = openFilePath_;
    const QString text = editor_->toPlainText();
    setStatus(tr("保存 %1 …").arg(path));
    saveWatcher_->setFuture(QtConcurrent::run(
        [root, path, text]() { return CoreBridge::writeFile(root, path, text, nullptr); }));
}

void MainWindow::refreshBranch() {
    if (workspaceRoot_.isEmpty()) {
        return;
    }
    const QString root = workspaceRoot_;
    branchWatcher_->setFuture(
        QtConcurrent::run([root]() { return CoreBridge::gitBranch(root); }));
}

void MainWindow::setStatus(const QString &message) {
    statusLabel_->setText(message);
}
