# Copyright (C) 2025 Nicholas Wierzbowski / Elbo Studio
# This file is part of the Pivot Bridge for Blender.

# Load common configuration (use the SDK-local copy so elbo-sdk can be used as a submodule)
include(${CMAKE_CURRENT_SOURCE_DIR}/../pivot-cmake-common/cmake/PivotCommon.cmake)

find_package(Python3 COMPONENTS Interpreter Development.Module NumPy REQUIRED)
find_program(CYTHON_EXECUTABLE cython REQUIRED)

function(elbo_sdk_cython_module_overrides target_name)
    if(target_name STREQUAL "shm_bridge")
        set(_elbo_cpp_src ${CMAKE_CURRENT_SOURCE_DIR}/../cpp/src)
        target_sources(${target_name} PRIVATE
            ${_elbo_cpp_src}/shm_bridge.cpp
            ${_elbo_cpp_src}/shm_segment.cpp
            ${_elbo_cpp_src}/uid.cpp
        )
        target_include_directories(${target_name} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../cpp/include)
        if(TARGET Boost::headers)
            target_link_libraries(${target_name} PRIVATE Boost::headers)
        endif()
        if(TARGET Boost::filesystem)
            target_link_libraries(${target_name} PRIVATE Boost::filesystem)
        endif()
        if(TARGET Boost::system)
            target_link_libraries(${target_name} PRIVATE Boost::system)
        endif()
    elseif(target_name STREQUAL "shm_manager")
        set(_elbo_cpp_src ${CMAKE_CURRENT_SOURCE_DIR}/../cpp/src)
        target_sources(${target_name} PRIVATE
            ${_elbo_cpp_src}/shm_manager_api.cpp
            ${_elbo_cpp_src}/uid.cpp
        )
        target_include_directories(${target_name} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../cpp/include)
    elseif(target_name STREQUAL "engine" AND TARGET elbo_sdk_cpp)
        target_include_directories(${target_name} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../cpp/include)
        target_link_libraries(${target_name} PRIVATE elbo_sdk_cpp)
    endif()
endfunction()

set(CYTHON_MODULE_SOURCES
    engine.pyx
    shm_manager.pyx
    shm_bridge.pyx
)

if(NOT DEFINED PIVOT_EDITION)
    set(PIVOT_EDITION "PRO" CACHE STRING "Edition to build (PRO or STANDARD)")
endif()
string(TOUPPER "${PIVOT_EDITION}" PIVOT_EDITION)
if(NOT PIVOT_EDITION STREQUAL "PRO" AND NOT PIVOT_EDITION STREQUAL "STANDARD")
    message(FATAL_ERROR "Invalid PIVOT_EDITION '${PIVOT_EDITION}'. Choose PRO or STANDARD.")
endif()

if(NOT DEFINED PIVOT_PLATFORM_ID)
    pivot_common_platform_id(PIVOT_PLATFORM_ID)
endif()

set(ELBO_SDK_WHEEL_PKG_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../lib")
set(ELBO_SDK_WHEEL_OUTPUT_DIR "${PROJECT_SOURCE_DIR}/dist/wheels/${PIVOT_PLATFORM_ID}/${PIVOT_EDITION}")

file(MAKE_DIRECTORY "${ELBO_SDK_WHEEL_PKG_DIR}")
file(MAKE_DIRECTORY "${ELBO_SDK_WHEEL_OUTPUT_DIR}")

set(_elbo_edition_defines)
if(PIVOT_EDITION STREQUAL "PRO")
    list(APPEND _elbo_edition_defines PIVOT_EDITION_PRO)
else()
    list(APPEND _elbo_edition_defines PIVOT_EDITION_STANDARD)
endif()
list(APPEND _elbo_edition_defines PIVOT_EDITION_NAME=\"${PIVOT_EDITION}\")

set(_cython_common_includes
    "${CMAKE_CURRENT_BINARY_DIR}"
    "${CMAKE_CURRENT_SOURCE_DIR}"
    "${CMAKE_CURRENT_SOURCE_DIR}/.."
    "${PROJECT_SOURCE_DIR}/pivot-core"
    ${Python3_INCLUDE_DIRS}
)
list(REMOVE_DUPLICATES _cython_common_includes)

set(_elbo_all_modules)
foreach(mod_src IN LISTS CYTHON_MODULE_SOURCES)
    get_filename_component(mod "${mod_src}" NAME_WE)
    set(SRC "${CMAKE_CURRENT_SOURCE_DIR}/${mod_src}")
    set(OUT "${CMAKE_CURRENT_BINARY_DIR}/${mod}.cpp")

    set(_cython_deps "${SRC}")
    set(PXD_FILE "${CMAKE_CURRENT_SOURCE_DIR}/${mod}.pxd")
    if(EXISTS "${PXD_FILE}")
        list(APPEND _cython_deps "${PXD_FILE}")
    endif()

    set(_cython_command "${CYTHON_EXECUTABLE}" --cplus)
    foreach(_inc_dir IN LISTS _cython_common_includes)
        list(APPEND _cython_command -I "${_inc_dir}")
    endforeach()
    list(APPEND _cython_command -o "${OUT}" "${SRC}")

    add_custom_command(
        OUTPUT "${OUT}"
        COMMAND ${_cython_command}
        DEPENDS ${_cython_deps}
        COMMENT "Cythonizing ${SRC}"
    )

    add_library(${mod} MODULE "${OUT}")
    list(APPEND _elbo_all_modules ${mod})

    elbo_sdk_cython_module_overrides(${mod})

    set_target_properties(${mod} PROPERTIES PREFIX "")
    if(WIN32)
        set_target_properties(${mod} PROPERTIES SUFFIX ".pyd")
    endif()

    target_link_libraries(${mod} PRIVATE Python3::Module)
    if(CMAKE_SYSTEM_NAME MATCHES "Linux")
        target_link_libraries(${mod} PRIVATE rt)
    endif()

    target_include_directories(${mod} PRIVATE ${Python3_INCLUDE_DIRS})
    if(Python3_NumPy_INCLUDE_DIRS)
        target_include_directories(${mod} PRIVATE ${Python3_NumPy_INCLUDE_DIRS})
    endif()

    foreach(def IN LISTS _elbo_edition_defines)
        target_compile_definitions(${mod} PRIVATE ${def})
    endforeach()

    set_target_properties(${mod} PROPERTIES LIBRARY_OUTPUT_DIRECTORY "${ELBO_SDK_WHEEL_PKG_DIR}")
endforeach()

add_custom_target(elbo_sdk_wheel ALL
    COMMAND ${Python3_EXECUTABLE} "${PROJECT_SOURCE_DIR}/build_wheel.py" --output-dir "${ELBO_SDK_WHEEL_OUTPUT_DIR}"
    WORKING_DIRECTORY "${PROJECT_SOURCE_DIR}"
    COMMENT "Building elbo_sdk wheel"
    DEPENDS ${_elbo_all_modules}
)