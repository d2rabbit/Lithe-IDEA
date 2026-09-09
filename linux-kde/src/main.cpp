#include "main_window.h"

#include <QApplication>

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    QApplication::setApplicationName(QStringLiteral("Lithe"));
    QApplication::setOrganizationName(QStringLiteral("lithe"));

    MainWindow window;

    // Optional positional argument: a workspace folder to open at launch,
    // mirroring the Windows host's CLI open flow.
    const QStringList arguments = QApplication::arguments();
    if (arguments.size() > 1) {
        window.loadWorkspace(arguments.at(1));
    }

    window.show();
    return QApplication::exec();
}
