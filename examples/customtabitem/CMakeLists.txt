cmake_minimum_required(VERSION 3.9 FATAL_ERROR)

option(QT6 "Target Qt 6" OFF)

project(customtabitem)

if(NOT QT6)
	set(CMAKE_OSX_ARCHITECTURES x86_64)
endif()

file(GLOB SOURCES
	*.cpp
	*.h)

if(DEMO)
	add_library(customtabitem STATIC ${SOURCES})
else()
	add_library(customtabitem SHARED ${SOURCES})
endif()

target_link_libraries(customtabitem binaryninjaapi binaryninjaui)

set_target_properties(customtabitem PROPERTIES
    CXX_STANDARD 17
	CXX_VISIBILITY_PRESET hidden
	CXX_STANDARD_REQUIRED ON
    VISIBILITY_INLINES_HIDDEN ON
	POSITION_INDEPENDENT_CODE ON)

if(BN_INTERNAL_BUILD)
	ui_plugin_rpath(customtabitem)
	set_target_properties(customtabitem PROPERTIES
		LIBRARY_OUTPUT_DIRECTORY ${BN_CORE_PLUGIN_DIR}
		RUNTIME_OUTPUT_DIRECTORY ${BN_CORE_PLUGIN_DIR})
endif()
