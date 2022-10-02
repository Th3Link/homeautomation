var content_update = document.getElementById("content_update");
var content_can_device = document.getElementById("content_can_device");
var content_can_device_console = document.getElementById("content_can_device_console");
var content_state = document.getElementById("content_state");
var content_restart = document.getElementById("content_restart");

var nav_save = document.getElementById("save");
var nav_state = document.getElementById("nav_state");
var nav_can_devices = document.getElementById("nav_can_devices");
var nav_update = document.getElementById("nav_update");

var setup_general_hostname = document.getElementById("general_hostname");
var setup_wifi_mode = document.getElementById("wifi_mode");
var setup_wifi_ssid = document.getElementById("wifi_ssid");
var setup_wifi_password = document.getElementById("wifi_password");
var setup_mqtt_uri = document.getElementById("mqtt_uri");
var setup_can_baudrate = document.getElementById("can_baudrate");

var state_refresh = document.getElementById("refresh");
var state_restart = document.getElementById("restart");

var devices_refresh = document.getElementById("refresh_devices");
var devices_broadcast_ping = document.getElementById("broadcast_ping");
var devices_query_all = document.getElementById("query_all");
var devices_restart_all = document.getElementById("restart_all");

var state = null;
var loaded_config = null;
var current_config = {};

var type_options = [];

function clearPressed() {
    nav_state.classList.remove("pressed");
    nav_can_devices.classList.remove("pressed");
    nav_update.classList.remove("pressed");
    content_update.classList.add("hide-me");
    content_state.classList.add("hide-me");
    content_can_device.classList.add("hide-me");
    content_can_device_console.classList.add("hide-me");
    content_restart.classList.add("hide-me");
}

nav_can_devices.addEventListener("click", function () {
    clearPressed();
    nav_can_devices.classList.add("pressed");
    updateDeviceList();

    content_can_device.classList.remove("hide-me");
    content_can_device_console.classList.remove("hide-me");
});

nav_update.addEventListener("click", function () {
    clearPressed();
    nav_update.classList.add("pressed");
    content_update.classList.remove("hide-me");
    content_update.innerHTML = "update";
});

setup_general_hostname.addEventListener("input", checkConfig)
setup_wifi_mode.addEventListener("input", checkConfig)
setup_wifi_ssid.addEventListener("input", checkConfig)
setup_wifi_password.addEventListener("input", checkConfig)
setup_mqtt_uri.addEventListener("input", checkConfig)
setup_can_baudrate.addEventListener("input", checkConfig)

nav_save.addEventListener("click", function () {
    if (!nav_save.classList.contains("deactivated")) {
        var xhr = new XMLHttpRequest();
        xhr.open("POST", '/config.json', true);
        xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        xhr.onreadystatechange = function () {
            if (this.readyState === XMLHttpRequest.DONE && this.status === 200) {
                get_config();
                // Request finished. Do processing here.
            } else {
                //error happend
            }
        };
        xhr.send(JSON.stringify(current_config));
    }
});


state_refresh.addEventListener("click", function () {
    update_state();
    get_config();
});

state_restart.addEventListener("click", function () {
    var xhr = new XMLHttpRequest();
    xhr.open("POST", '/control.json', true);
    xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
    xhr.onreadystatechange = function () {
        if (this.readyState === XMLHttpRequest.DONE && this.status === 200) {
            // Request finished. Do processing here.
            clearPressed();
            content_restart.classList.remove("hide-me");
        } else {
            //error happend
        }
    };

    const restart_command = { command: "restart", unit: "self" };

    xhr.send(JSON.stringify(restart_command));
});

nav_state.click();