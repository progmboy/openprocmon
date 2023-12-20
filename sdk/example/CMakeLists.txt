project(example)

if (CMAKE_CXX_COMPILER_ID STREQUAL "MSVC")
    set(CMAKE_CXX_FLAGS_RELEASE "${CMAKE_CXX_FLAGS_RELEASE} /MT")
    set(CMAKE_CXX_FLAGS_DEBUG "${CMAKE_CXX_FLAGS_DEBUG} /MTd")
endif()

add_definitions(-DUNICODE)
add_definitions(-D_UNICODE)

#add inc file search dir
INCLUDE_DIRECTORIES(
    "${OPENPROCMON_SOURCE_DIR}/sdk/procmonsdk"
)

add_executable(example 
    example.cpp
)

set_target_properties(example PROPERTIES LINK_FLAGS "/MANIFESTUAC:\"level='requireAdministrator' uiAccess='false'\"")
target_link_libraries(example libprocmon)