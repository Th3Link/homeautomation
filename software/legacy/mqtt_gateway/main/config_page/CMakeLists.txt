cmake_minimum_required(VERSION 3.16)
project(can2mqtt_gui)

# JavaScript Bundle einbetten
add_custom_command(
  OUTPUT ${CMAKE_CURRENT_SOURCE_DIR}/src/bundle.js
  COMMAND node ${CMAKE_CURRENT_SOURCE_DIR}/build.js
  DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/src/js/app.js
           ${CMAKE_CURRENT_SOURCE_DIR}/src/js/api.js
           # Alle Komponenten als Dependencies auflisten
)

add_custom_target(bundle ALL DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/src/bundle.js)

# Einbetten der Web-Ressourcen
embed_file(src/index.html)
embed_file(src/bundle.js)
embed_file(src/style.css)  # Falls vorhanden
