# Copyright (C) 2025 Nicholas Wierzbowski / Elbo Studio
# This file is part of the Pivot Bridge for Blender.

# Load common configuration so the SDK can be used as a standalone package.
include(${pivot-cmake-common_SOURCE_DIR}/cmake/PivotCommon.cmake)

pivot_common_cython_find_python()

set(CYTHON_MODULE_SOURCES
    engine.pyx
    shm_bridge.pyx
    shm_manager.pyx
)

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
list(APPEND _elbo_edition_defines PIVOT_EDITION_NAME="${PIVOT_EDITION}")

set(_elbo_cython_includes
    "${CMAKE_CURRENT_BINARY_DIR}"
    "${CMAKE_CURRENT_SOURCE_DIR}"
    "${CMAKE_CURRENT_SOURCE_DIR}/.."
    "${CMAKE_CURRENT_SOURCE_DIR}/../cpp/include"
    ${Python3_INCLUDE_DIRS}
)
if(Python3_NumPy_INCLUDE_DIRS)
    list(APPEND _elbo_cython_includes ${Python3_NumPy_INCLUDE_DIRS})
endif()
if(_Python3_NumPy_INCLUDE_DIR)
    list(APPEND _elbo_cython_includes ${_Python3_NumPy_INCLUDE_DIR})
endif()
list(REMOVE_DUPLICATES _elbo_cython_includes)

pivot_common_cython_create_modules(
    MODULE_SOURCES ${CYTHON_MODULE_SOURCES}
    PYX_ROOT "${CMAKE_CURRENT_SOURCE_DIR}"
    BINARY_DIR "${CMAKE_CURRENT_BINARY_DIR}"
    INCLUDE_DIRS ${_elbo_cython_includes}
    DEFINES ${_elbo_edition_defines}
    WHEEL_PKG_DIR "${ELBO_SDK_WHEEL_PKG_DIR}"
    MODULE_TARGETS_VAR _elbo_sdk_modules
)

pivot_common_cython_create_wheel_target(
    TARGET_NAME elbo_sdk_wheel
    SCRIPT_PATH "${PROJECT_SOURCE_DIR}/build_wheel.py"
    OUTPUT_DIR "${ELBO_SDK_WHEEL_OUTPUT_DIR}"
    MODULE_TARGETS ${_elbo_sdk_modules}
)

foreach(_module IN LISTS _elbo_sdk_modules)
    target_link_libraries(${_module} PRIVATE elbo_sdk_cpp)
endforeach()
