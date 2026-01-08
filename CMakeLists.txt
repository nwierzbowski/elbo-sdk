cmake_minimum_required(VERSION 3.16)

project(elbo_sdk LANGUAGES CXX)

# Load common Pivot configuration
include(${CMAKE_CURRENT_SOURCE_DIR}/pivot-cmake-common/PivotCommon.cmake)

# Build toggles
option(BUILD_PY_MODULE "Build Python (Cython) extension" ON)

# Prepare output directories
_pivot_prepare_dir("${PIVOT_BIN_DIR}" "${PIVOT_PLATFORM_ID}")

# Setup Boost for IPC
_pivot_setup_boost()

# -----------------------------------------------------------------------------
# Native SDK library (C++)
# -----------------------------------------------------------------------------

add_library(elbo_sdk_cpp
  ${CMAKE_CURRENT_SOURCE_DIR}/cpp/shm_bridge.cpp
  ${CMAKE_CURRENT_SOURCE_DIR}/cpp/engine_client.cpp
)

set_target_properties(elbo_sdk_cpp PROPERTIES POSITION_INDEPENDENT_CODE ON)

target_compile_features(elbo_sdk_cpp PUBLIC cxx_std_17)

target_include_directories(elbo_sdk_cpp
  PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/include
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp
)

if(Boost_FOUND)
  target_include_directories(elbo_sdk_cpp PUBLIC ${Boost_INCLUDE_DIRS})
elseif(boost_ext_POPULATED)
  target_include_directories(elbo_sdk_cpp PUBLIC ${boost_ext_SOURCE_DIR})
endif()

if(TARGET Boost::json)
  target_link_libraries(elbo_sdk_cpp PUBLIC Boost::json)
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