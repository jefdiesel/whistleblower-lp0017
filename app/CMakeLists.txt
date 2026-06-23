# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Whistleblower — Logos Core "ui_qml" module build.
#
# This follows the logos-module-builder convention: the build is driven by the
# `logos_module()` CMake macro provided by logos-module-builder (made available
# via the Nix dev/build environment, see flake.nix). That macro wires the module
# into Logos Core (reads app/metadata.json, links the Logos module ABI, and emits
# the artifact that the Nix `.lgx` packaging step bundles).
#
# NOTE: the exact `logos_module()` signature / available helpers depend on the
# installed logos-module-builder version. Keyword args below (NAME/TYPE/VIEW/
# SOURCES/QML_FILES/METADATA/QML_IMPORT_NAME...) reflect the documented
# convention; align them with the version pinned in flake.nix if the configure
# step complains (e.g. some versions take positional args or a single TARGET).

cmake_minimum_required(VERSION 3.21)

project(whistleblower
    VERSION 0.1.0
    DESCRIPTION "Censorship-resistant document upload (Logos Basecamp ui_qml app)"
    LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)   # WhistleblowerBackend uses Q_OBJECT
set(CMAKE_AUTORCC ON)

# Qt6: Quick/Qml for the UI, Core for the backend helper.
find_package(Qt6 REQUIRED COMPONENTS Core Qml Quick)

# logos-module-builder ships its CMake package; it is on CMAKE_PREFIX_PATH inside
# the Nix environment. This provides the logos_module() macro used below.
find_package(LogosModuleBuilder QUIET)
if(NOT COMMAND logos_module)
    # Some logos-module-builder versions expose the macro via include() of a
    # bundled .cmake file rather than find_package(). Try that fallback so this
    # file is robust across versions; the env var is set by flake.nix.
    if(DEFINED ENV{LOGOS_MODULE_BUILDER_CMAKE})
        include($ENV{LOGOS_MODULE_BUILDER_CMAKE})
    endif()
endif()

set(WB_SOURCES
    src/WhistleblowerBackend.cpp
    src/WhistleblowerBackend.h
)

set(WB_QML_FILES
    qml/Main.qml
)

# Preferred path: let logos-module-builder define and package the module.
# It is expected to:
#   * build a module target from WB_SOURCES,
#   * embed the QML files (qml/) as resources,
#   * read app/metadata.json (type=ui_qml, view=qml/Main.qml),
#   * make the artifact available to the Nix `.lgx` packaging step.
if(COMMAND logos_module)
    logos_module(
        NAME            whistleblower
        TYPE            ui_qml
        METADATA        ${CMAKE_CURRENT_SOURCE_DIR}/metadata.json
        VIEW            qml/Main.qml
        SOURCES         ${WB_SOURCES}
        QML_FILES       ${WB_QML_FILES}
        QML_IMPORT_NAME Whistleblower            # QML: `import Whistleblower 1.0`
        QML_IMPORT_MAJOR_VERSION 1
        QT_COMPONENTS   Core Qml Quick
    )
else()
    # --- Fallback (no logos-module-builder available) ----------------------
    # Lets the C++/QML compile and the backend be unit-checkable off-platform;
    # this does NOT produce a loadable .lgx (that requires logos-module-builder).
    message(WARNING
        "logos_module() not found — building a plain Qt library target only. "
        "Use the Nix environment (flake.nix) to produce a loadable .lgx module.")

    qt_add_library(whistleblower STATIC ${WB_SOURCES})
    target_link_libraries(whistleblower PRIVATE Qt6::Core Qt6::Qml Qt6::Quick)

    # Expose WhistleblowerBackend to QML as `import Whistleblower 1.0`.
    qt_add_qml_module(whistleblower
        URI Whistleblower
        VERSION 1.0
        QML_FILES ${WB_QML_FILES}
    )
endif()

# Register the backend as a QML-creatable type. With qt_add_qml_module the type
# can also be exported via QML_ELEMENT; we register explicitly so it works under
# either build path. (qmlRegisterType is called from the module's C++ init; if
# logos-module-builder provides its own registration hook, prefer that.)
#
# Equivalent at runtime to, in C++:
#   qmlRegisterType<WhistleblowerBackend>("Whistleblower", 1, 0, "WhistleblowerBackend");
