# Copyright (C) 2025 Nicholas Wierzbowski / Elbo Studio
# This file is part of the Pivot Bridge for Blender.

# Load common configuration (use the SDK-local copy so elbo-sdk can be used as a submodule)
include(${CMAKE_CURRENT_SOURCE_DIR}/../pivot-cmake-common/PivotCommon.cmake)

# Setup Cython using common macro
_pivot_setup_cython()

function(elbo_sdk_cython_module_overrides target_name)
    if(target_name STREQUAL "shm_bridge")
        set(_elbo_cpp_src ${CMAKE_CURRENT_SOURCE_DIR}/../cpp/src)
        target_sources(${target_name} PRIVATE
            ${_elbo_cpp_src}/shm_bridge.cpp
            ${_elbo_cpp_src}/shm_segment.cpp
            ${_elbo_cpp_src}/uid.cpp
        )
        target_include_directories(${target_name} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../cpp/include)
        if(Boost_FOUND)
            target_include_directories(${target_name} PRIVATE ${Boost_INCLUDE_DIRS})
        elseif(boost_ext_POPULATED)
            target_include_directories(${target_name} PRIVATE ${boost_ext_SOURCE_DIR})
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

# Build all Cython modules using common macro
if(NOT DEFINED PIVOT_ELBO_WHEEL_OUTPUT_DIR)
    set(PIVOT_ELBO_WHEEL_OUTPUT_DIR "${CMAKE_SOURCE_DIR}/dist")
endif()
_pivot_add_cython_modules("${CYTHON_MODULE_SOURCES}" "${CMAKE_CURRENT_SOURCE_DIR}/../lib" "${PIVOT_ELBO_WHEEL_OUTPUT_DIR}" elbo_sdk_cython_module_overrides)