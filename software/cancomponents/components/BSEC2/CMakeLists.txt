set(COMPONENT_SRCS 
"bsec_integration.c")

set(COMPONENT_ADD_INCLUDEDIRS include)
set(COMPONENT_REQUIRES "BME680")
register_component()
target_link_libraries(${COMPONENT_TARGET} "-L${CMAKE_CURRENT_LIST_DIR}/lib")
target_link_libraries(${COMPONENT_TARGET} algobsec)
