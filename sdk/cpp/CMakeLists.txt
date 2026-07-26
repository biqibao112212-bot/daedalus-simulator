cmake_minimum_required(VERSION 3.16)
project(DaedalusSimSdk VERSION 1.1.0 LANGUAGES CXX)

# MSVC emits one program database per target. /FS serializes access when
# CMake builds multiple translation units in parallel.
add_compile_options($<$<CXX_COMPILER_ID:MSVC>:/FS>)

include(CMakePackageConfigHelpers)
include(CTest)
find_package(Threads REQUIRED)

add_library(DaedalusSimSdk STATIC
  src/scene_control_client.cpp
  src/runtime_capabilities.cpp
  src/socket_platform.cpp
  src/talos_metadata_reader.cpp
  src/tcp_image_client.cpp
  src/udp_gimbal_client.cpp
)
add_library(DaedalusSimSdk::DaedalusSimSdk ALIAS DaedalusSimSdk)
set_target_properties(DaedalusSimSdk PROPERTIES
  OUTPUT_NAME daedalus_sim_sdk
  POSITION_INDEPENDENT_CODE ON
)
target_compile_features(DaedalusSimSdk PUBLIC cxx_std_17)
target_include_directories(DaedalusSimSdk PUBLIC
  $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
  $<INSTALL_INTERFACE:include>
)
target_link_libraries(DaedalusSimSdk PUBLIC Threads::Threads)
if(WIN32)
  target_link_libraries(DaedalusSimSdk PUBLIC ws2_32)
endif()

if(BUILD_TESTING)
  add_executable(daedalus_sim_sdk_layout_test tests/layout_test.cpp)
  target_link_libraries(daedalus_sim_sdk_layout_test PRIVATE
    DaedalusSimSdk::DaedalusSimSdk
  )
  add_test(NAME daedalus_sim_sdk_layout_test
    COMMAND daedalus_sim_sdk_layout_test
  )
  add_executable(daedalus_sim_sdk_tcp_image_test tests/tcp_image_test.cpp)
  target_link_libraries(daedalus_sim_sdk_tcp_image_test PRIVATE
    DaedalusSimSdk::DaedalusSimSdk
  )
  add_test(NAME daedalus_sim_sdk_tcp_image_test
    COMMAND daedalus_sim_sdk_tcp_image_test
  )

  add_executable(daedalus_sim_sdk_scene_control_test
    tests/scene_control_test.cpp
  )
  target_link_libraries(daedalus_sim_sdk_scene_control_test PRIVATE
    DaedalusSimSdk::DaedalusSimSdk
  )
  add_test(NAME daedalus_sim_sdk_scene_control_test
    COMMAND daedalus_sim_sdk_scene_control_test
  )

  add_executable(daedalus_sim_sdk_talos_reader_test
    tests/talos_reader_test.cpp
  )
  target_link_libraries(daedalus_sim_sdk_talos_reader_test PRIVATE
    DaedalusSimSdk::DaedalusSimSdk
  )
  add_test(NAME daedalus_sim_sdk_talos_reader_test
    COMMAND daedalus_sim_sdk_talos_reader_test
  )

  add_executable(daedalus_sim_sdk_gimbal_test
    tests/udp_gimbal_test.cpp
  )
  target_link_libraries(daedalus_sim_sdk_gimbal_test PRIVATE
    DaedalusSimSdk::DaedalusSimSdk
  )
  add_test(NAME daedalus_sim_sdk_gimbal_test
    COMMAND daedalus_sim_sdk_gimbal_test
  )

  add_executable(daedalus_sim_sdk_tcp_client_test
    tests/tcp_image_client_test.cpp
  )
  target_link_libraries(daedalus_sim_sdk_tcp_client_test PRIVATE
    DaedalusSimSdk::DaedalusSimSdk
  )
  add_test(NAME daedalus_sim_sdk_tcp_client_test
    COMMAND daedalus_sim_sdk_tcp_client_test
  )

  add_executable(daedalus_sim_sdk_runtime_capabilities_test
    tests/runtime_capabilities_test.cpp
  )
  target_link_libraries(daedalus_sim_sdk_runtime_capabilities_test PRIVATE
    DaedalusSimSdk::DaedalusSimSdk
  )
  add_test(NAME daedalus_sim_sdk_runtime_capabilities_test
    COMMAND daedalus_sim_sdk_runtime_capabilities_test
  )
endif()

install(TARGETS DaedalusSimSdk EXPORT DaedalusSimSdkTargets
  ARCHIVE DESTINATION lib
  LIBRARY DESTINATION lib
  RUNTIME DESTINATION bin
)
install(DIRECTORY include/ DESTINATION include)
install(EXPORT DaedalusSimSdkTargets
  FILE DaedalusSimSdkTargets.cmake
  NAMESPACE DaedalusSimSdk::
  DESTINATION lib/cmake/DaedalusSimSdk
)

configure_package_config_file(
  cmake/DaedalusSimSdkConfig.cmake.in
  ${CMAKE_CURRENT_BINARY_DIR}/DaedalusSimSdkConfig.cmake
  INSTALL_DESTINATION lib/cmake/DaedalusSimSdk
)
write_basic_package_version_file(
  ${CMAKE_CURRENT_BINARY_DIR}/DaedalusSimSdkConfigVersion.cmake
  VERSION ${PROJECT_VERSION}
  COMPATIBILITY SameMajorVersion
)
install(FILES
  ${CMAKE_CURRENT_BINARY_DIR}/DaedalusSimSdkConfig.cmake
  ${CMAKE_CURRENT_BINARY_DIR}/DaedalusSimSdkConfigVersion.cmake
  DESTINATION lib/cmake/DaedalusSimSdk
)
