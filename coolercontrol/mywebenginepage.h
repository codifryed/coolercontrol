#ifndef MYWEBENGINEPAGE_H
#define MYWEBENGINEPAGE_H

#include <iostream>
#include <QWebEnginePage>
#include <QDesktopServices>

class MyWebEnginePage final : public QWebEnginePage {
    Q_OBJECT

public:
    using QWebEnginePage::QWebEnginePage;

    // bool acceptNavigationRequest(const QUrl &url, const QWebEnginePage::NavigationType type,
    //                              const bool isMainFrame) override {
    //     std::cout << "Navigation request: " << url.toString().toStdString() << " : " << isMainFrame << " : " << type << std::endl;
    //     if (type == QWebEnginePage::NavigationTypeLinkClicked) {
    //         QDesktopServices::openUrl(url);
    //         return false;
    //     }
    //     return true;
    // }

    // QWebEnginePage* createWindow(QWebEnginePage::WebWindowType) override {
    //     return nullptr;
    //     // return this;
    // }
};

#endif //MYWEBENGINEPAGE_H
