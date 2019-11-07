cmake_minimum_required(VERSION 3.1)
project(openprocmon)

##
## set the cmake moudle path
## to find wdk and wtl
##

set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_CURRENT_SOURCE_DIR}/cmake")

SET(LIBRARY_OUTPUT_PATH "${PROJECT_BINARY_DIR}/lib")
set(EXECUTABLE_OUTPUT_PATH "${PROJECT_BINARY_DIR}/bin")

add_subdirectory(kernel)
add_subdirectory(gui)
add_subdirectory(sdk/procmonsdk)