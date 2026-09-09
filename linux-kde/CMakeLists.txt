cmake_minimum_required(VERSION 3.24)
project(lithe-kde VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 REQUIRED COMPONENTS Widgets Concurrent)

set(LITHE_CORE_LIB
    "${CMAKE_CURRENT_SOURCE_DIR}/../rust/target/debug/liblithe_core.a"
    CACHE FILEPATH "Prebuilt lithe-core static library")

add_executable(lithe-kde
    src/main.cpp
    src/core_bridge.cpp
    src/main_window.cpp)

target_include_directories(lithe-kde PRIVATE src)
target_link_libraries(lithe-kde PRIVATE
    Qt6::Widgets
    Qt6::Concurrent
    ${LITHE_CORE_LIB}
    pthread
    dl
    m)
