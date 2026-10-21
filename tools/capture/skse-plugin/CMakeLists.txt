cmake_minimum_required(VERSION 3.24)
project(MudcrabCapture VERSION 0.1.0 LANGUAGES CXX)

# CommonLibSSE-NG (alandtse fork, GPL-3.0-or-later with the Modding and Linking Exceptions) is read from
# the folder named by COMMONLIBSSE_PATH, a plain clone of https://github.com/alandtse/CommonLibSSE-NG
# at release v12.1.0 or later (the first line with 1.7.104 addresses).
if(NOT DEFINED ENV{COMMONLIBSSE_PATH} AND NOT COMMONLIBSSE_PATH)
  message(FATAL_ERROR "Set COMMONLIBSSE_PATH to a CommonLibSSE-NG checkout")
endif()
if(NOT COMMONLIBSSE_PATH)
  set(COMMONLIBSSE_PATH "$ENV{COMMONLIBSSE_PATH}")
endif()

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>DLL")

# Only the 1.6.629+ / 1.7 layouts (Anniversary Edition runtimes) are built: the plugin targets 1.7.104.
set(BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(ENABLE_SKYRIM_SE OFF CACHE BOOL "" FORCE)
set(ENABLE_SKYRIM_VR OFF CACHE BOOL "" FORCE)
set(COMMONLIB_ENABLE_IPO OFF CACHE BOOL "" FORCE)
add_subdirectory("${COMMONLIBSSE_PATH}" CommonLibSSE EXCLUDE_FROM_ALL)
include("${COMMONLIBSSE_PATH}/cmake/CommonLibSSE.cmake")  # defines add_commonlibsse_plugin

add_commonlibsse_plugin(${PROJECT_NAME}
  AUTHOR "Mudcrab capture tooling"
  VERSION ${PROJECT_VERSION}
  SOURCES src/main.cpp)

target_precompile_headers(${PROJECT_NAME} PRIVATE src/PCH.h)
target_link_libraries(${PROJECT_NAME} PRIVATE d3d11 dxgi windowscodecs ole32)
target_compile_definitions(${PROJECT_NAME} PRIVATE WIN32_LEAN_AND_MEAN NOMINMAX)
