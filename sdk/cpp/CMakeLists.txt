cmake_minimum_required(VERSION 3.16)
project(DaedalusSimSdk VERSION 1.0.0 LANGUAGES CXX)

include(CMakePackageConfigHelpers)
include(CTest)

add_library(DaedalusSimSdk INTERFACE)
add_library(DaedalusSimSdk::DaedalusSimSdk ALIAS DaedalusSimSdk)
target_compile_features(DaedalusSimSdk INTERFACE cxx_std_17)
target_include_directories(DaedalusSimSdk INTERFACE
  $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
  $<INSTALL_INTERFACE:include>
)

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
endif()

install(TARGETS DaedalusSimSdk EXPORT DaedalusSimSdkTargets)
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
