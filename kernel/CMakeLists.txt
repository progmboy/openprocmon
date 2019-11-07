project(procmon)

set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} /W4 /WX /wd4996")
set(CMAKE_EXE_LINKER_FLAGS "${CMAKE_EXE_LINKER_FLAGS} /INTEGRITYCHECK")

find_package(WDK REQUIRED)

file(GLOB DRIVER_HEADER *.h)
file(GLOB DRIVER_SOURCES *.c)

#source_group("inc" FILES ${DRIVER_HEADER})
#source_group("source" FILES ${DRIVER_SOURCES})
set(DRIVER_RC procmon.rc)

wdk_add_driver(procmon
    ${DRIVER_HEADER}
    ${DRIVER_SOURCES}
    ${DRIVER_RC}
)


target_link_libraries(procmon WDK::FLTMGR WDK::WDMSEC)