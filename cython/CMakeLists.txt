# Copyright (C) 2025 Nicholas Wierzbowski / Elbo Studio
# This file is part of the Pivot Bridge for Blender.
# Find python for Cython module

#Find and add cython_cmake to module path
if(NOT PIVOT_DEV)
execute_process(
    COMMAND "${Python_EXECUTABLE}" -c "import cython_cmake, os; print(os.path.join(os.path.dirname(cython_cmake.__file__), 'cmake'))"
    OUTPUT_VARIABLE CYTHON_CMAKE_DIR
    OUTPUT_STRIP_TRAILING_WHITESPACE
)

# Add that directory to your search path
list(APPEND CMAKE_MODULE_PATH "${CYTHON_CMAKE_DIR}")
endif()


find_package(Python COMPONENTS Interpreter Development.Module NumPy REQUIRED)
find_package(Cython REQUIRED MODULE)
include(UseCython)

set(ELBO_SDK_STAGING_DIR "${CMAKE_CURRENT_BINARY_DIR}/staging/elbo_sdk")

# Ensure the __init__.py updates in the build folder whenever you edit the source
configure_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/../elbo_sdk/__init__.py"
    "${ELBO_SDK_STAGING_DIR}/__init__.py"
    COPYONLY
)

# Set defines for C++ code based on flags

set(ELBO_SDK_MODULE_NAMES
    engine
    shm_bridge
    shm_manager
)

set(_elbo_sdk_targets)
foreach(_module IN LISTS ELBO_SDK_MODULE_NAMES)
    set(_module_src "${CMAKE_CURRENT_SOURCE_DIR}/${_module}.pyx")

    cython_transpile("${_module_src}" LANGUAGE CXX OUTPUT_VARIABLE _generated_c)

    python_add_library(${_module} MODULE WITH_SOABI ${_generated_c})
    set_target_properties(${_module} PROPERTIES
        LIBRARY_OUTPUT_DIRECTORY "${ELBO_SDK_STAGING_DIR}"
    )
    target_link_libraries(${_module} PRIVATE elbo_sdk_cpp Python::Module Python::NumPy pivot_common::base_cython)
    install(TARGETS ${_module} LIBRARY DESTINATION elbo_sdk)
    list(APPEND _elbo_sdk_targets ${_module})
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