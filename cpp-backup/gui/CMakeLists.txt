project(openprocmingui)

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

find_package(WTL REQUIRED)
INCLUDE_DIRECTORIES(${WTL_INCLUDE_DIR})

file(GLOB GUI_HEADER *.h)
file(GLOB GUI_SOURCES *.cpp)
file(GLOB GUI_RC_SOURCES *.rc)

add_executable(openprocmingui 
    WIN32
    ${GUI_HEADER} 
    ${GUI_SOURCES} 
    ${GUI_RC_SOURCES}
)

set_target_properties(openprocmingui PROPERTIES LINK_FLAGS "/MANIFESTUAC:\"level='requireAdministrator' uiAccess='false'\"")
target_link_libraries(openprocmingui libprocmon)