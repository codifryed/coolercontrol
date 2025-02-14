cmake_minimum_required(VERSION 3.19)
project(coolercontrol LANGUAGES CXX)

find_package(Qt6 6.5 REQUIRED COMPONENTS Core Widgets WebEngineWidgets WebChannel)

qt_standard_project_setup()

qt_add_executable(
        coolercontrol
        #    WIN32 MACOSX_BUNDLE
        main.cpp
        mainwindow.cpp
        mainwindow.h
        mywebenginepage.h
)

target_link_libraries(coolercontrol
        PRIVATE
        Qt::Core
        Qt::Widgets
        Qt::WebEngineWidgets
        Qt::WebChannel
)

# Resources:
set(cc_resource_files
        "icons/icon.ico"
        "icons/icon.png"
)

qt_add_resources(coolercontrol "coolercontrol"
        PREFIX "/"
#        BASE "icons"
        FILES ${cc_resource_files}
)

include(GNUInstallDirs)

install(TARGETS coolercontrol
        BUNDLE DESTINATION .
        RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
        LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
)

qt_generate_deploy_app_script(
        TARGET coolercontrol
        OUTPUT_SCRIPT deploy_script
        NO_UNSUPPORTED_PLATFORM_ERROR
)
install(SCRIPT ${deploy_script})
