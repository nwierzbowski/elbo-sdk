# Copyright (C) 2025 Nicholas Wierzbowski / Elbo Studio
# This file is part of the Pivot Bridge for Blender.
# Find python for Cython module
find_package(Python COMPONENTS Interpreter Development.Module NumPy REQUIRED)

set(ELBO_SDK_STAGING_DIR "${CMAKE_CURRENT_BINARY_DIR}/staging/elbo_sdk")

# Ensure the __init__.py updates in the build folder whenever you edit the source
configure_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/../elbo_sdk/__init__.py"
    "${ELBO_SDK_STAGING_DIR}/__init__.py"
    COPYONLY
)

# Set defines for C++ code based on flags
set(_elbo_edition_defines)
if(PIVOT_EDITION STREQUAL "PRO")
    list(APPEND _elbo_edition_defines PIVOT_EDITION_PRO)
else()
    list(APPEND _elbo_edition_defines PIVOT_EDITION_STANDARD)
endif()
list(APPEND _elbo_edition_defines PIVOT_EDITION_NAME="${PIVOT_EDITION}")

set(ELBO_SDK_MODULE_NAMES
    engine
    shm_bridge
    shm_manager
)

set(_elbo_sdk_targets)
foreach(_module IN LISTS ELBO_SDK_MODULE_NAMES)
    set(_module_src "${CMAKE_CURRENT_SOURCE_DIR}/${_module}.pyx")
    python_add_library(elbo_sdk_${_module} MODULE WITH_SOABI ${_module_src})
    set_target_properties(elbo_sdk_${_module} PROPERTIES
        LIBRARY_OUTPUT_DIRECTORY "${ELBO_SDK_STAGING_DIR}"
    )
    target_link_libraries(elbo_sdk_${_module} PRIVATE elbo_sdk_cpp Python::Module Python::NumPy)

    target_compile_definitions(elbo_sdk_${_module} PRIVATE ${_elbo_edition_defines})
    target_compile_features(elbo_sdk_${_module} PRIVATE cxx_std_17)
    install(TARGETS elbo_sdk_${_module} LIBRARY DESTINATION elbo_sdk)
    list(APPEND _elbo_sdk_targets elbo_sdk_${_module})
endforeach()


install(FILES "${CMAKE_CURRENT_SOURCE_DIR}/../elbo_sdk/__init__.py" DESTINATION elbo_sdk)

add_custom_target(elbo_sdk_cython
    DEPENDS ${_elbo_sdk_targets}
)

# Symlink to Blender site-packages for development
pivot_common_cython_add_blender_symlink(
    TARGET_NAME elbo_sdk_cython
    SOURCE_PATH "${ELBO_SDK_STAGING_DIR}"
    PACKAGE_NAME elbo_sdk
    BLENDER_SITE_PACKAGES "${BLENDER_SITE_PACKAGES}"
)