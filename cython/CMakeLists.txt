# Copyright (C) 2025 Nicholas Wierzbowski / Elbo Studio
# This file is part of the Pivot Bridge for Blender.

# Load common configuration (use the SDK-local copy so elbo-sdk can be used as a submodule)
include(${CMAKE_CURRENT_SOURCE_DIR}/../pivot-cmake-common/PivotCommon.cmake)

# Setup Cython using common macro
_pivot_setup_cython()

set(CYTHON_MODULE_SOURCES
    shared/engine.pyx
    shared/shm_manager.pyx
    shared/shm_bridge.pyx
)

# Build all Cython modules using common macro
if(NOT DEFINED PIVOT_ELBO_WHEEL_OUTPUT_DIR)
    set(PIVOT_ELBO_WHEEL_OUTPUT_DIR "${CMAKE_SOURCE_DIR}/dist/wheels")
endif()
_pivot_add_cython_modules("${CYTHON_MODULE_SOURCES}" "${CMAKE_CURRENT_SOURCE_DIR}/../src/elbo_sdk" "${PIVOT_ELBO_WHEEL_OUTPUT_DIR}")