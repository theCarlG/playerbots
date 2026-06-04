#
# Copyright (C) 2005-2018 MaNGOS project <http://getmangos.com/>
#
# This program is free software; you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation; either version 2 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program; if not, write to the Free Software
# Foundation, Inc., 59 Temple Place, Suite 330, Boston, MA  02111-1307  USA
#

set(LIBRARY_NAME playerbots)

#Add PCH Files
set(bots_PCH "${CMAKE_CURRENT_SOURCE_DIR}/botpch.h")

# ── Rust AI module ─────────────────────────────────────────────────────────
# Select expansion feature flag matching the CMaNGOS project.
if(${CMAKE_PROJECT_NAME} MATCHES "Classic")
    set(RUST_FEATURE "vanilla")
elseif(${CMAKE_PROJECT_NAME} MATCHES "TBC")
    set(RUST_FEATURE "tbc")
else()
    set(RUST_FEATURE "wotlk")
endif()

# Profile flag for Cargo
if(CMAKE_BUILD_TYPE STREQUAL "Release" OR CMAKE_BUILD_TYPE STREQUAL "RelWithDebInfo")
    set(CARGO_PROFILE "--release")
    set(RUST_TARGET_DIR "release")
else()
    set(CARGO_PROFILE "")
    set(RUST_TARGET_DIR "debug")
endif()

set(RUST_WORKSPACE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/playerbot-rs")
set(RUST_LIB "${RUST_WORKSPACE_DIR}/target/${RUST_TARGET_DIR}/libplayerbot_rs.a")

add_custom_target(playerbot_rs_build ALL
    COMMAND cargo build -p playerbot ${CARGO_PROFILE} --features ${RUST_FEATURE}
    WORKING_DIRECTORY ${RUST_WORKSPACE_DIR}
    COMMENT "Building Rust playerbot (feature=${RUST_FEATURE})"
    BYPRODUCTS ${RUST_LIB}
)

add_library(playerbot_rs_lib STATIC IMPORTED GLOBAL)
set_target_properties(playerbot_rs_lib PROPERTIES
    IMPORTED_LOCATION ${RUST_LIB}
)
add_dependencies(playerbot_rs_lib playerbot_rs_build)

# ── C++ wrapper sources ────────────────────────────────────────────────────
set(CppWrapper_Source
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/BotBridge.h
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/BotBridge.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/BotConfig.h
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/BotConfig.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/ItemBridge.h
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/ItemBridge.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/LoginBridge.h
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/LoginBridge.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/RandomFactoryBridge.h
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/RandomFactoryBridge.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/RandomMgrBridge.h
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/RandomMgrBridge.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/RandomPlayerbotMgr.h
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/RandomPlayerbotMgr.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/MgrBridge.h
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/MgrBridge.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/playerbotDefs.h
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/playerbot.h
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/PlayerbotAI.h
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/PlayerbotAIBase.h
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/PlayerbotAIBase.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/CoreStubs.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/PlayerbotRust.h
    ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper/PlayerbotRust.cpp
)
source_group("Rust Wrapper" FILES ${CppWrapper_Source})

#Base files
file(GLOB Playerbot_Source ${CMAKE_CURRENT_SOURCE_DIR}/playerbot/*.cpp ${CMAKE_CURRENT_SOURCE_DIR}/playerbot/*.h)
    source_group("Player Bot" FILES ${Playerbot_Source})

file(GLOB AHbot_Source ${CMAKE_CURRENT_SOURCE_DIR}/ahbot/*.cpp ${CMAKE_CURRENT_SOURCE_DIR}/ahbot/*.h)
    source_group("AH Bot" FILES ${AHbot_Source})

#Define base source library
set(LIBRARY_SRCS
    ${Playerbot_Source}
    ${AHbot_Source}
    ${CppWrapper_Source}
    ${bots_PCH}
    ${CMAKE_CURRENT_SOURCE_DIR}/botpch.cpp
)

# NOTE: The old strategy engine (playerbot/strategy/) has been removed.
# All AI behavior is now provided by the Rust module (playerbot-rs/) via
# the cpp_wrapper.  See REWRITE.md for the full migration plan.
#
# Remaining playerbot/ C++ files are utility infrastructure that is still
# used by PlayerbotMgr, RandomPlayerbotMgr, and other management code.
# These will be progressively cleaned up as integration validation proceeds.

# Define ENABLE_PLAYERBOTS if need
if (BUILD_PLAYERBOTS)
  add_definitions(-DENABLE_PLAYERBOTS)
endif()

# Define Core
if ( ${CMAKE_PROJECT_NAME} MATCHES "CMaNGOS")
  add_definitions(-DCMANGOS)
else()
  if ( ${CMAKE_PROJECT_NAME} MATCHES "MaNGOS")
    add_definitions(-DMANGOS)
  endif()
endif()

# Define Expansion
if ( ${CMAKE_PROJECT_NAME} MATCHES "Classic")
  add_definitions(-DMANGOSBOT_ZERO)
endif()
if ( ${CMAKE_PROJECT_NAME} MATCHES "TBC")
  add_definitions(-DMANGOSBOT_ONE)
endif()
if ( ${CMAKE_PROJECT_NAME} MATCHES "WoTLK")
  add_definitions(-DMANGOSBOT_TWO)
endif()

# Define ENABLE_MODULES if need
if (BUILD_MODULES)
  add_definitions(-DENABLE_MODULES)
endif()

# Define BUILD_AHBOT if need
if (BUILD_AHBOT)
  add_definitions(-DBUILD_AHBOT)
endif()

if(PCH)
  include_directories(${CMAKE_CURRENT_BINARY_DIR})
endif()

add_library(${LIBRARY_NAME} STATIC ${LIBRARY_SRCS})

target_link_libraries(${LIBRARY_NAME}
  PRIVATE shared
  PRIVATE Detour
  PRIVATE g3dlite
  PRIVATE zlib
  PRIVATE playerbot_rs_lib
)
if(WIN32)
  target_link_libraries(${LIBRARY_NAME}
    PRIVATE ws2_32
  )
else()
  # Rust static lib needs pthread and dl on Linux/macOS
  target_link_libraries(${LIBRARY_NAME}
    PRIVATE pthread
    PRIVATE dl
  )
endif()

target_include_directories(${LIBRARY_NAME}
  PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/cpp_wrapper
)

target_include_directories(${LIBRARY_NAME}
  PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}
)

# Install config files
if ( ${CMAKE_PROJECT_NAME} MATCHES "Classic")
	configure_file(${CMAKE_CURRENT_SOURCE_DIR}/playerbot/aiplayerbot.conf.dist.in ${CMAKE_CURRENT_BINARY_DIR}/aiplayerbot.conf.dist)
endif()	
if ( ${CMAKE_PROJECT_NAME} MATCHES "TBC")
	configure_file(${CMAKE_CURRENT_SOURCE_DIR}/playerbot/aiplayerbot.conf.dist.in.tbc ${CMAKE_CURRENT_BINARY_DIR}/aiplayerbot.conf.dist)
endif()	
if ( ${CMAKE_PROJECT_NAME} MATCHES "WoTLK")
	configure_file(${CMAKE_CURRENT_SOURCE_DIR}/playerbot/aiplayerbot.conf.dist.in.wotlk ${CMAKE_CURRENT_BINARY_DIR}/aiplayerbot.conf.dist)
endif()
if (NOT CONF_INSTALL_DIR)
  set(CONF_INSTALL_DIR ${CONF_DIR})
endif()
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/aiplayerbot.conf.dist DESTINATION ${CONF_INSTALL_DIR})

set_target_properties(${LIBRARY_NAME} PROPERTIES PROJECT_LABEL "PlayerBots")

# Generate PCH
if(PCH)
  target_precompile_headers(${LIBRARY_NAME} PRIVATE ${bots_PCH})
endif()
