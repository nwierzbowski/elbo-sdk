from pathlib import Path
import os
import shutil
from setuptools import Extension, setup
from setuptools.command.build_ext import build_ext


class PrecompiledBuildExt(build_ext):
    def __init__(self, *args, **kwargs):
        self.src_dir = kwargs.pop('src_dir', None)
        super().__init__(*args, **kwargs)
        
    def build_extension(self, ext):
        if not ext.sources:
            # Precompiled, copy the .so from src_dir to the target
            mod_name = ext.name.split('.')[-1]
            if self.src_dir:
                src = self.src_dir / f"{mod_name}.so"
            else:
                src = Path(mod_name).with_suffix('.so')
            target = self.get_ext_fullpath(ext.name)
            os.makedirs(os.path.dirname(target), exist_ok=True)
            shutil.copy2(src, target)
            print(f"Copied {src} to {target}")
        else:
            raise RuntimeError(f"Extension {ext.name} has sources but this setup.py only handles precompiled modules")


def _discover_compiled_modules(pkg_dir: Path) -> list[str]:
    # CMake drops compiled modules into this package directory as <name>.so/.pyd.
    module_files = [*pkg_dir.glob("*.so"), *pkg_dir.glob("*.pyd")]
    return sorted({p.stem for p in module_files})


def _make_stub_extensions(mod_names: list[str]) -> list[Extension]:
    return [Extension(f"elbo_sdk.{name}", sources=[]) for name in mod_names]


def main() -> None:
    pkg_dir = Path(__file__).parent
    # CMake places compiled modules into the project's `lib/` directory.
    # Use that location so precompiled modules are discovered without copying.
    src_dir = pkg_dir.parent / "lib"
    mod_names = _discover_compiled_modules(src_dir)
    extensions = _make_stub_extensions(mod_names)

    setup(ext_modules=extensions, cmdclass={'build_ext': lambda *args, **kwargs: PrecompiledBuildExt(*args, src_dir=src_dir, **kwargs)})


if __name__ == "__main__":
    main()
