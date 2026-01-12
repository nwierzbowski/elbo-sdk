cmake_minimum_required(VERSION 4.2.1)

project(elbo_sdk LANGUAGES CXX)

include(FetchContent)

FetchContent_Declare(
    pivot-cmake-common
    GIT_REPOSITORY https://github.com/nwierzbowski/pivot-cmake-common.git  # Replace with actual repo URL
    GIT_TAG main  # Replace with appropriate tag or commit
)

FetchContent_MakeAvailable(pivot-cmake-common)

# Load common Pivot configuration
include(${pivot-cmake-common_SOURCE_DIR}/cmake/PivotCommon.cmake)

setup_common_interfaces()
set_common_edition(PIVOT_EDITION)
pivot_common_platform_id(PIVOT_PLATFORM_ID)

add_subdirectory(cpp)
add_subdirectory(cython)

# Convenience meta-target
if(NOT TARGET dev)
  add_custom_target(dev ALL)
endif()

add_dependencies(dev elbo_sdk_cython)
add_dependencies(elbo_sdk_cython elbo_sdk_cpp)
