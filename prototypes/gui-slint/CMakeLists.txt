cmake_minimum_required(VERSION 3.21)

project(moonlight-gui-slint-prototype LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Slint CONFIG REQUIRED)

add_executable(moonlight-gui-slint-prototype
    main.cpp
)

slint_target_sources(moonlight-gui-slint-prototype moonlight_shell.slint)

target_link_libraries(moonlight-gui-slint-prototype
    PRIVATE
        Slint::Slint
)
