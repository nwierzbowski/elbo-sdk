cmake_minimum_required(VERSION 3.16)

project(elbo_sdk LANGUAGES CXX)

# Load common Pivot configuration
include(${CMAKE_CURRENT_SOURCE_DIR}/pivot-cmake-common/cmake/PivotCommon.cmake)

pivot_common_define_targets()

if(NOT DEFINED PIVOT_EDITION)
  set(PIVOT_EDITION "PRO" CACHE STRING "Edition to build (PRO or STANDARD)")
endif()
set_property(CACHE PIVOT_EDITION PROPERTY STRINGS PRO STANDARD)
string(TOUPPER "${PIVOT_EDITION}" PIVOT_EDITION)
if(NOT PIVOT_EDITION STREQUAL "PRO" AND NOT PIVOT_EDITION STREQUAL "STANDARD")
  message(FATAL_ERROR "Invalid PIVOT_EDITION '${PIVOT_EDITION}'. Choose PRO or STANDARD.")
endif()

pivot_common_platform_id(PIVOT_PLATFORM_ID)

# Build toggles
option(BUILD_PY_MODULE "Build Python (Cython) extension" ON)

# Setup Boost for IPC
pivot_common_setup_boost(COMPONENTS interprocess filesystem system json)

# -----------------------------------------------------------------------------
# Native SDK library (C++)
# -----------------------------------------------------------------------------

add_library(elbo_sdk_cpp
  ${CMAKE_CURRENT_SOURCE_DIR}/cpp/src/shm_bridge.cpp
  ${CMAKE_CURRENT_SOURCE_DIR}/cpp/src/engine_client.cpp
  ${CMAKE_CURRENT_SOURCE_DIR}/cpp/src/engine_api.cpp
  ${CMAKE_CURRENT_SOURCE_DIR}/cpp/src/uid.cpp
  ${CMAKE_CURRENT_SOURCE_DIR}/cpp/src/shm_manager_api.cpp
  ${CMAKE_CURRENT_SOURCE_DIR}/cpp/src/shm_segment.cpp
)

set_target_properties(elbo_sdk_cpp PROPERTIES POSITION_INDEPENDENT_CODE ON)

target_compile_features(elbo_sdk_cpp PUBLIC cxx_std_17)

target_include_directories(elbo_sdk_cpp
  PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp/include
)

if(TARGET Boost::headers)
  target_link_libraries(elbo_sdk_cpp PUBLIC Boost::headers)
endif()

if(TARGET Boost::json)
  target_link_libraries(elbo_sdk_cpp PUBLIC Boost::json)
endif()

if(TARGET Boost::filesystem)
  target_link_libraries(elbo_sdk_cpp PUBLIC Boost::filesystem)
endif()
if(TARGET Boost::system)
  target_link_libraries(elbo_sdk_cpp PUBLIC Boost::system)
endif()
if(TARGET Boost::process)
  target_link_libraries(elbo_sdk_cpp PUBLIC Boost::process)
endif()

# Build Cython Subdirectory
if(BUILD_PY_MODULE)
    if(EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/cython/CMakeLists.txt)
        add_subdirectory(cython)
    endif()
endif()

# Convenience meta-target
if(NOT TARGET dev)
  add_custom_target(dev ALL)
  if(TARGET bridge)
    add_dependencies(dev bridge)
  endif()
endif()