project(libprocmon)

if (CMAKE_CXX_COMPILER_ID STREQUAL "MSVC")
    set(CMAKE_CXX_FLAGS_RELEASE "${CMAKE_CXX_FLAGS_RELEASE} /MT")
    set(CMAKE_CXX_FLAGS_DEBUG "${CMAKE_CXX_FLAGS_DEBUG} /MTd")
endif()

add_definitions(-DUNICODE)
add_definitions(-D_UNICODE)

#add inc file search dir
INCLUDE_DIRECTORIES(
    "${OPENPROCMON_SOURCE_DIR}/kernel"
)

file(GLOB SDK_HEADER *.hpp)
file(GLOB SDK_SOURCES *.cxx)

source_group("inc" FILES ${SDK_HEADER})
source_group("source" FILES ${SDK_SOURCES})

ADD_LIBRARY(libprocmon STATIC
    ${SDK_HEADER}
    ${SDK_SOURCES}
)