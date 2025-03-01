cmake_minimum_required(VERSION 3.19)
project(coolercontrol LANGUAGES CXX)

find_package(Qt6 REQUIRED COMPONENTS Core Widgets WebEngineWidgets WebChannel)

# This required Qt6.3+, and jammy is still 6.2: qt_standard_project_setup() 6.2
# version:
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_AUTOUIC ON)

qt_add_executable(
  coolercontrol
  # WIN32 MACOSX_BUNDLE
  main.cpp
  main_window.cpp
  main_window.h
  address_wizard.cpp
  address_wizard.h
  ipc.cpp
  ipc.h)

target_link_libraries(coolercontrol PRIVATE Qt::Core Qt::Widgets
                                            Qt::WebEngineWidgets Qt::WebChannel)

# Resources:
set(cc_resource_files "icons/icon.ico" "icons/icon.png")

qt_add_resources(
  coolercontrol "coolercontrol" PREFIX "/"
  # BASE "icons"
  FILES ${cc_resource_files})

include(GNUInstallDirs)

install(
  TARGETS coolercontrol
  BUNDLE DESTINATION .
  RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
  LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR})

# requires Qt 6.3+ qt_generate_deploy_app_script( TARGET coolercontrol
# OUTPUT_SCRIPT deploy_script NO_UNSUPPORTED_PLATFORM_ERROR ) install(SCRIPT
# ${deploy_script})
