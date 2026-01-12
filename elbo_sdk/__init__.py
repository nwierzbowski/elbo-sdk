# Copyright (C) 2026 Nicholas Wierzbowski / Elbo Studio
"""
Elbo SDK: Platform-independent bridge for engine IPC and shared memory.
"""

# 1. Versioning & Metadata
__version__ = "1.0.1"
# This will be available if you used the compile definitions in CMake
try:
    from . import elbo_sdk_engine as _engine
    __edition__ = getattr(_engine, "PIVOT_EDITION_NAME", "UNKNOWN")
    del _engine # Clean up the temporary import name
except ImportError:
    __edition__ = "DEVELOPMENT"
    warnings.warn("Elbo SDK running in DEVELOPMENT mode (compiled modules not found).")

# 2. Expose the Compiled Modules
# Since CMake names the files 'elbo_sdk_engine.so', but they are inside 
# the 'elbo_sdk' folder, we provide clean aliases.

try:
    # We import the compiled members so users can do: 
    # 'from elbo_sdk import engine'
    from . import elbo_sdk_engine as engine
    from . import elbo_sdk_shm_bridge as _shm_bridge
    from . import elbo_sdk_shm_manager as shm_manager
except ImportError as e:
    # Useful for debugging Blender console issues
    import warnings
    warnings.warn(f"Failed to load compiled Elbo SDK modules: {e}")

# 3. Define what is visible when someone does 'from elbo_sdk import *'
__all__ = [
    "engine",
    "shm_manager",
    "__version__",
    "__edition__",
]
