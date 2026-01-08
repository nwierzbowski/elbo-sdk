from pathlib import Path
import os
import shutil
from setuptools import Extension, setup
from setuptools.command.build_ext import build_ext


class PrecompiledBuildExt(build_ext):
    def build_extension(self, ext):
        if not ext.sources:
            # Precompiled, copy the .so to the target
            mod_name = ext.name.split('.')[-1]
            src = Path(mod_name).with_suffix('.so')
            target = self.get_ext_fullpath(ext.name)
            os.makedirs(os.path.dirname(target), exist_ok=True)
            shutil.copy2(src, target)
            print(f"Copied {src} to {target}")
        else:
            super().build_extension(ext)


def _discover_compiled_modules(pkg_dir: Path) -> list[str]:
    # CMake drops compiled modules into this package directory as <name>.so/.pyd.
    module_files = [*pkg_dir.glob("*.so"), *pkg_dir.glob("*.pyd")]
    return sorted({p.stem for p in module_files})


def _make_stub_extensions(mod_names: list[str]) -> list[Extension]:
    return [Extension(f"elbo_sdk.{name}", sources=[]) for name in mod_names]


def main() -> None:
    pkg_dir = Path(__file__).parent
    mod_names = _discover_compiled_modules(pkg_dir)
    extensions = _make_stub_extensions(mod_names)

    setup(ext_modules=extensions, cmdclass={'build_ext': PrecompiledBuildExt})


if __name__ == "__main__":
    main()
