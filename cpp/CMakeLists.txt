add_library(elbo_sdk_cpp
  src/shm_bridge.cpp
  src/engine_client.cpp
  src/engine_api.cpp
  src/uid.cpp
  src/shm_manager_api.cpp
  src/shm_segment.cpp
)

set_target_properties(elbo_sdk_cpp PROPERTIES POSITION_INDEPENDENT_CODE ON)

target_compile_features(elbo_sdk_cpp PUBLIC cxx_std_17)

target_include_directories(elbo_sdk_cpp
  PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
)

# Setup Boost for IPC
pivot_common_setup_boost(COMPONENTS interprocess filesystem system json)


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